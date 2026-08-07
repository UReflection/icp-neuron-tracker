use serde::{Deserialize, Serialize};
use std::fs;

// Custom deserializer for large neuron IDs
fn deserialize_neuron_ids<'de, D>(deserializer: D) -> Result<Vec<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrU64 {
        String(String),
        U64(u64),
    }

    let items: Vec<StringOrU64> = Vec::deserialize(deserializer)?;

    items
        .into_iter()
        .map(|item| match item {
            StringOrU64::String(s) => s.parse::<u64>().map_err(D::Error::custom),
            StringOrU64::U64(n) => Ok(n),
        })
        .collect()
}

// Custom serializer for neuron IDs (serialize as strings to preserve large numbers)
fn serialize_neuron_ids<S>(ids: &Vec<u64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;
    let mut seq = serializer.serialize_seq(Some(ids.len()))?;
    for id in ids {
        seq.serialize_element(&id.to_string())?;
    }
    seq.end()
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub identity: IdentityConfig,
    pub ic: IcConfig,
    pub neurons: NeuronsConfig,
    pub tracking: TrackingConfig,
    pub display: DisplayConfig,
    #[serde(default)]
    pub retirement: RetirementConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IdentityConfig {
    pub pem_file: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IcConfig {
    pub governance_canister: String,
    pub ic_url: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct NeuronsConfig {
    #[serde(deserialize_with = "deserialize_neuron_ids")]
    #[serde(serialize_with = "serialize_neuron_ids")]
    pub ids: Vec<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TrackingConfig {
    pub history_file: String,  // Changed from dead_code
    pub snapshot_on_run: bool,  // Changed from dead_code
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DisplayConfig {
    pub show_individual_neurons: bool,
    pub show_portfolio_summary: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RetirementConfig {
    pub default_target_income: Option<f64>,
}

impl Default for RetirementConfig {
    fn default() -> Self {
        Self {
            default_target_income: None,
        }
    }
}

impl Config {
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Retained but unused. `project` no longer writes config as a side effect of running a
    /// projection; see the note in main.rs. Any future writer should preserve comments rather
    /// than re-serialising the whole file, which this does not.
    #[allow(dead_code)]
    pub fn save(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Retained but unused. The `retirement.default_target_income` field is still read from
    /// existing config files, but nothing consumes it and no command writes it.
    #[allow(dead_code)]
    pub fn update_retirement_target(&mut self, target: f64) {
        self.retirement.default_target_income = Some(target);
    }
}