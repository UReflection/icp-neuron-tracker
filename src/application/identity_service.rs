use crate::infrastructure::{GeneratedIdentity, IcClient, IdentityClient};
use crate::domain::{NeuronId};
use std::path::Path;

/// Service for managing identity operations
pub struct IdentityService;

/// Information about an identity
#[derive(Debug)]
pub struct IdentityInfo {
    pub principal: String,
    pub pem_path: String,
}

/// Authorization status for a neuron
#[derive(Debug)]
pub struct NeuronAuthStatus {
    pub neuron_id: String,
    pub is_authorized: bool,
    pub error: Option<String>,
}

impl IdentityService {
    pub fn new() -> Self {
        Self
    }

    /// Generate new Secp256k1 identity and save to file
    pub fn generate_identity(
        &self,
        output_path: &Path,
    ) -> Result<IdentityInfo, Box<dyn std::error::Error>> {
        // Generate identity
        let generated: GeneratedIdentity = IdentityClient::generate_identity()?;

        // Write PEM file with restrictive permissions
        IdentityClient::write_pem_file(output_path, &generated.pem_content)?;

        Ok(IdentityInfo {
            principal: generated.principal,
            pem_path: output_path.to_string_lossy().to_string(),
        })
    }

    /// Get information about identity from PEM file
    pub fn get_identity_info(
        &self,
        pem_path: &Path,
    ) -> Result<IdentityInfo, Box<dyn std::error::Error>> {
        let principal = IdentityClient::get_principal(pem_path)?;

        Ok(IdentityInfo {
            principal,
            pem_path: pem_path.to_string_lossy().to_string(),
        })
    }

    /// Verify identity can access neurons
    pub async fn verify_neuron_access(
        &self,
        pem_path: &Path,
        neuron_ids: &[String],
        ic_url: &str,
        governance_canister: &str,
    ) -> Result<Vec<NeuronAuthStatus>, Box<dyn std::error::Error>> {
        let mut results = Vec::new();

        // Create IC client with identity
        let ic_client = IcClient::new(
            pem_path.to_str().ok_or("Invalid PEM path")?,
            ic_url,
            governance_canister,
        )?;

        // Check each neuron
        for neuron_id_str in neuron_ids {
            let neuron_id_u64: u64 = neuron_id_str
                .parse()
                .map_err(|_| format!("Invalid neuron ID: {}", neuron_id_str))?;

            let neuron_id = NeuronId::new(neuron_id_u64);
            let status = match ic_client.fetch_neuron(neuron_id).await {
                Ok(_) => NeuronAuthStatus {
                    neuron_id: neuron_id_str.clone(),
                    is_authorized: true,
                    error: None,
                },
                Err(e) => {
                    let error_msg = e.to_string();
                    let is_auth_error = error_msg.contains("not authorized")
                        || error_msg.contains("permission")
                        || error_msg.contains("Caller not authorized");

                    NeuronAuthStatus {
                        neuron_id: neuron_id_str.clone(),
                        is_authorized: false,
                        error: Some(if is_auth_error {
                            "Not authorized as hot key".to_string()
                        } else {
                            error_msg
                        }),
                    }
                }
            };

            results.push(status);
        }

        Ok(results)
    }
}

impl Default for IdentityService {
    fn default() -> Self {
        Self::new()
    }
}