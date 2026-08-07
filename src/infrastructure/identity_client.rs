use ic_agent::identity::Secp256k1Identity;
use ic_agent::Identity;
use k256::SecretKey;
use sec1::LineEnding; // For SEC1 format
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

/// Client for managing Secp256k1 identities
pub struct IdentityClient;

/// Represents a newly generated identity
#[derive(Debug)]
pub struct GeneratedIdentity {
    pub principal: String,
    pub pem_content: String,
}

impl IdentityClient {
    pub fn new() -> Self {
        Self
    }

    /// Generate new Secp256k1 identity
    pub fn generate_identity() -> Result<GeneratedIdentity, Box<dyn std::error::Error>> {
        // Generate random Secp256k1 keypair
        let secret_key = SecretKey::random(&mut rand::thread_rng());

        // Create IC identity from secret key
        let identity = Secp256k1Identity::from_private_key(secret_key.clone());

        // Derive principal
        let principal = identity.sender()?.to_text();

        // Export to PEM format using SEC1 (required by ic-agent)
        let pem_content = secret_key.to_sec1_pem(LineEnding::LF)?;

        Ok(GeneratedIdentity {
            principal,
            pem_content: pem_content.to_string(),
        })
    }

    /// Write PEM content to file with restrictive permissions
    pub fn write_pem_file(
        path: &Path,
        pem_content: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;

        file.write_all(pem_content.as_bytes())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = file.metadata()?;
            let mut perms = metadata.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(path, perms)?;
        }

        Ok(())
    }

    /// Load Secp256k1 identity from PEM file
    pub fn load_identity(
        pem_path: &Path,
    ) -> Result<Secp256k1Identity, Box<dyn std::error::Error>> {
        let mut file = File::open(pem_path).map_err(|e| {
            format!("Cannot open PEM file '{}': {}", pem_path.display(), e)
        })?;

        let mut pem_content = String::new();
        file.read_to_string(&mut pem_content).map_err(|e| {
            format!("Cannot read PEM file '{}': {}", pem_path.display(), e)
        })?;

        Secp256k1Identity::from_pem(pem_content.as_bytes()).map_err(|e| {
            if pem_content.contains("BEGIN PRIVATE KEY")
                && !pem_content.contains("BEGIN EC PRIVATE KEY")
            {
                format!(
                    "PEM file '{}' uses PKCS#8 format.\n\
                     This tracker requires SEC1 format (BEGIN EC PRIVATE KEY).\n\
                     Generate new identity: icp-neuron-tracker identity generate\n\
                     Original error: {}",
                    pem_path.display(),
                    e
                )
                .into()
            } else {
                format!("Cannot parse PEM file '{}': {}", pem_path.display(), e).into()
            }
        })
    }

    /// Get principal from PEM file
    pub fn get_principal(pem_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
        let identity = Self::load_identity(pem_path)?;
        let principal = identity.sender()?.to_text();
        Ok(principal)
    }

    /// Validate PEM file
    #[allow(dead_code)]
    pub fn validate_pem(pem_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        Self::load_identity(pem_path)?;
        Ok(())
    }
}

impl Default for IdentityClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_secp256k1_identity() {
        let result = IdentityClient::generate_identity();
        assert!(result.is_ok());

        let identity = result.unwrap();
        assert!(identity.principal.contains('-'));
        assert!(identity.principal.len() > 20);
        assert!(identity.pem_content.contains("BEGIN"));
        assert!(identity.pem_content.contains("END"));
    }

    #[test]
    fn test_write_and_load_identity() {
        let temp_dir = TempDir::new().unwrap();
        let pem_path = temp_dir.path().join("test.pem");

        let generated = IdentityClient::generate_identity().unwrap();
        IdentityClient::write_pem_file(&pem_path, &generated.pem_content).unwrap();

        assert!(pem_path.exists());

        let loaded_principal = IdentityClient::get_principal(&pem_path).unwrap();
        assert_eq!(generated.principal, loaded_principal);
    }

    #[test]
    fn test_validate_pem() {
        let temp_dir = TempDir::new().unwrap();
        let pem_path = temp_dir.path().join("test.pem");

        let generated = IdentityClient::generate_identity().unwrap();
        IdentityClient::write_pem_file(&pem_path, &generated.pem_content).unwrap();

        assert!(IdentityClient::validate_pem(&pem_path).is_ok());
    }

    #[test]
    fn test_invalid_pem() {
        let temp_dir = TempDir::new().unwrap();
        let pem_path = temp_dir.path().join("invalid.pem");

        std::fs::write(&pem_path, "not a valid pem").unwrap();

        assert!(IdentityClient::validate_pem(&pem_path).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn test_file_permissions_unix() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let pem_path = temp_dir.path().join("test.pem");

        let generated = IdentityClient::generate_identity().unwrap();
        IdentityClient::write_pem_file(&pem_path, &generated.pem_content).unwrap();

        let metadata = std::fs::metadata(&pem_path).unwrap();
        let permissions = metadata.permissions();

        assert_eq!(permissions.mode() & 0o777, 0o600);
    }
}