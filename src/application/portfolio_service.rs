use crate::domain::{Portfolio, NeuronId};
use crate::infrastructure::IcClient;

pub struct PortfolioService {
    ic_client: IcClient,
}

#[derive(Debug)]
pub struct FetchResult {
    pub portfolio: Portfolio,
    pub errors: Vec<(NeuronId, String)>,
}

impl PortfolioService {
    pub fn new(ic_client: IcClient) -> Self {
        Self { ic_client }
    }

    pub async fn fetch_portfolio(&self, neuron_ids: &[NeuronId]) -> Result<FetchResult, Box<dyn std::error::Error>> {
        let mut neurons = Vec::new();
        let mut errors = Vec::new();

        for &neuron_id in neuron_ids.iter() {
            match self.ic_client.fetch_neuron(neuron_id).await {
                Ok(neuron) => {
                    neurons.push(neuron);
                }
                Err(e) => {
                    errors.push((neuron_id, e.to_string()));
                }
            }
        }

        Ok(FetchResult {
            portfolio: Portfolio::new(neurons),
            errors,
        })
    }
}