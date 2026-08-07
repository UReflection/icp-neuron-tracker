use candid::{CandidType, Deserialize, Encode, Decode};
use ic_agent::{Agent, identity::Secp256k1Identity, export::Principal as AgentPrincipal};
use crate::domain::{Neuron, NeuronId, IcpAmount, NeuronState};

// IC API types
#[allow(dead_code)]
#[derive(CandidType, Deserialize, Debug)]
struct NeuronIdProto {
    id: u64,
}

#[allow(dead_code)]
#[derive(CandidType, Deserialize, Debug)]
enum DissolveState {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}

#[derive(CandidType, Deserialize, Debug)]
struct CompleteNeuron {
    cached_neuron_stake_e8s: u64,
    maturity_e8s_equivalent: u64,
    staked_maturity_e8s_equivalent: Option<u64>,
    created_timestamp_seconds: u64,
    auto_stake_maturity: Option<bool>,
}

#[derive(CandidType, Deserialize, Debug)]
struct NeuronInfo {
    retrieved_at_timestamp_seconds: u64,
    state: i32,
    age_seconds: u64,
    dissolve_delay_seconds: u64,
    voting_power: u64,
    created_timestamp_seconds: u64,
}

#[derive(CandidType, Deserialize, Debug)]
struct GovernanceError {
    error_message: String,
    error_type: i32,
}

#[derive(CandidType, Deserialize, Debug)]
enum FullNeuronResult {
    Ok(CompleteNeuron),
    Err(GovernanceError),
}

#[derive(CandidType, Deserialize, Debug)]
enum NeuronInfoResult {
    Ok(NeuronInfo),
    Err(GovernanceError),
}

pub struct IcClient {
    agent: Agent,
    governance_canister: AgentPrincipal,
}

impl IcClient {
    pub fn new(
        pem_path: &str,
        ic_url: &str,
        governance_canister: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let pem_content = std::fs::read_to_string(pem_path)?;
        let identity = Secp256k1Identity::from_pem(pem_content.as_bytes())?;
        
        let agent = Agent::builder()
            .with_url(ic_url)
            .with_identity(identity)
            .build()?;
        
        let governance_canister = AgentPrincipal::from_text(governance_canister)?;
        
        Ok(Self {
            agent,
            governance_canister,
        })
    }

    pub async fn fetch_neuron(&self, neuron_id: NeuronId) -> Result<Neuron, Box<dyn std::error::Error>> {
        let id_value = neuron_id.value();
        
        // Query full neuron data
        let full_response = self.agent
            .update(&self.governance_canister, "get_full_neuron")
            .with_arg(Encode!(&id_value)?)
            .call_and_wait()
            .await?;
        
        let full_result: FullNeuronResult = Decode!(&full_response, FullNeuronResult)?;
        
        // Query neuron info
        let info_response = self.agent
            .query(&self.governance_canister, "get_neuron_info")
            .with_arg(Encode!(&id_value)?)
            .call()
            .await?;
        
        let info_result: NeuronInfoResult = Decode!(&info_response, NeuronInfoResult)?;
        
        match (full_result, info_result) {
            (FullNeuronResult::Ok(neuron_data), NeuronInfoResult::Ok(info)) => {
                Ok(Neuron::new(
                    neuron_id,
                    IcpAmount::from_e8s(neuron_data.cached_neuron_stake_e8s),
                    IcpAmount::from_e8s(neuron_data.maturity_e8s_equivalent),
                    IcpAmount::from_e8s(neuron_data.staked_maturity_e8s_equivalent.unwrap_or(0)),
                    info.voting_power,
                    info.age_seconds,
                    info.dissolve_delay_seconds,
                    NeuronState::from(info.state),
                    neuron_data.auto_stake_maturity.unwrap_or(false),
                    neuron_data.created_timestamp_seconds,
                ))
            }
            (FullNeuronResult::Err(e), _) | (_, NeuronInfoResult::Err(e)) => {
                Err(format!("Governance error for neuron {}: {}", neuron_id, e.error_message).into())
            }
        }
    }

    #[allow(dead_code)]
    pub async fn fetch_many(&self, neuron_ids: &[NeuronId]) -> Vec<Result<Neuron, Box<dyn std::error::Error>>> {
        let mut results = Vec::new();
        
        for &neuron_id in neuron_ids {
            results.push(self.fetch_neuron(neuron_id).await);
        }
        
        results
    }
}