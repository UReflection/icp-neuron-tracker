use csv::StringRecord;
use crate::domain::{Neuron, NeuronId, IcpAmount, NeuronState};
use chrono::NaiveDate;
use std::path::Path;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};

/// Represents a parsed CSV record containing a neuron snapshot
#[derive(Debug, Clone)]
pub struct CsvRecord {
    pub neuron_id: NeuronId,
    pub date: NaiveDate,
    pub neuron: Neuron,
}

/// Metadata for a neuron parsed from CSV comments
#[derive(Debug, Clone)]
struct NeuronMetadata {
    created_date: NaiveDate,
    dissolve_delay_years: u64,
}

/// CSV parser for importing historical neuron snapshots
pub struct CsvParser;

impl CsvParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse a CSV file containing neuron snapshot history
    ///
    /// Expected CSV format (FS-003 compliant):
    ///
    /// Simplified format with metadata:
    /// # neuron_id: 10000000000000000001, created_date: 2021-05-17, dissolve_delay_years: 8
    /// neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,voting_power,age_bonus_multiplier
    ///
    /// Traditional format:
    /// neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,available_maturity_e8s,
    /// voting_power,dissolve_delay_seconds,age_seconds,created_timestamp_seconds
    ///
    /// Comment lines starting with # are automatically skipped (or used for metadata)
    pub fn parse_file(&self, path: &Path) -> Result<Vec<CsvRecord>, Box<dyn std::error::Error>> {
        // First pass: Extract metadata from comments
        let metadata = self.parse_metadata(path)?;

        // Second pass: Parse CSV records
        let mut reader = csv::ReaderBuilder::new()
            .comment(Some(b'#'))
            .from_path(path)?;
        let headers = reader.headers()?.clone();

        // Validate headers (supports both simplified and traditional formats)
        self.validate_headers(&headers)?;

        let mut records = Vec::new();
        for (row_num, result) in reader.records().enumerate() {
            let record = result.map_err(|e| {
                format!("Error reading CSV at row {}: {}", row_num + 2, e)
            })?;

            let csv_record = self.parse_record(&headers, &record, row_num + 2, &metadata)?;
            records.push(csv_record);
        }

        Ok(records)
    }

    /// Validate that CSV headers match the expected format
    /// Supports both simplified format (with metadata) and traditional format
    fn validate_headers(&self, headers: &StringRecord) -> Result<(), Box<dyn std::error::Error>> {
        let required_headers = vec!["neuron_id", "snapshot_date", "stake_e8s", "staked_maturity_e8s", "voting_power"];

        // Check if this is simplified format (has age_bonus_multiplier) or traditional format
        let has_age_bonus = headers.iter().any(|h| h == "age_bonus_multiplier");
        let has_age_seconds = headers.iter().any(|h| h == "age_seconds");

        if has_age_bonus {
            // Simplified format: require age_bonus_multiplier, metadata provides the rest
            for required in &required_headers {
                if !headers.iter().any(|h| h == *required) {
                    return Err(format!(
                        "Missing required column '{}' in CSV header.\n\
                        For simplified format, expected headers: neuron_id, snapshot_date, stake_e8s, \
                        staked_maturity_e8s, voting_power, age_bonus_multiplier\n\
                        Note: available_maturity_e8s is optional (defaults to 0)",
                        required
                    ).into());
                }
            }

            if !headers.iter().any(|h| h == "age_bonus_multiplier") {
                return Err("Simplified format requires 'age_bonus_multiplier' column".into());
            }
        } else if has_age_seconds {
            // Traditional format: require all fields
            let traditional_headers = vec![
                "neuron_id",
                "snapshot_date",
                "stake_e8s",
                "staked_maturity_e8s",
                "available_maturity_e8s",
                "voting_power",
                "dissolve_delay_seconds",
                "age_seconds",
                "created_timestamp_seconds",
            ];

            for expected in &traditional_headers {
                if !headers.iter().any(|h| h == *expected) {
                    return Err(format!(
                        "Missing required column '{}' in CSV header.\n\
                        For traditional format, expected headers: {}",
                        expected,
                        traditional_headers.join(", ")
                    ).into());
                }
            }
        } else {
            return Err(
                "CSV format not recognized. Must have either:\n\
                - Simplified format: age_bonus_multiplier column + metadata comments\n\
                - Traditional format: age_seconds, dissolve_delay_seconds, created_timestamp_seconds columns".into()
            );
        }

        Ok(())
    }

    /// Parse neuron metadata from CSV comment lines
    /// Expected format: # neuron_id: 10000000000000000001, created_date: 2021-05-17, dissolve_delay_years: 8
    fn parse_metadata(&self, path: &Path) -> Result<HashMap<u64, NeuronMetadata>, Box<dyn std::error::Error>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut metadata_map = HashMap::new();

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();

            // Skip non-comment lines and empty comments
            if !trimmed.starts_with('#') || trimmed.len() <= 1 {
                continue;
            }

            // Remove leading '#' and trim
            let content = trimmed[1..].trim();

            // Look for metadata pattern: "neuron_id: X, created_date: Y, dissolve_delay_years: Z"
            if !content.contains("neuron_id:") {
                continue;
            }

            // Parse the metadata fields
            let mut neuron_id: Option<u64> = None;
            let mut created_date: Option<NaiveDate> = None;
            let mut dissolve_delay_years: Option<u64> = None;

            for part in content.split(',') {
                let part = part.trim();

                if let Some(value) = part.strip_prefix("neuron_id:") {
                    neuron_id = value.trim().parse().ok();
                } else if let Some(value) = part.strip_prefix("created_date:") {
                    created_date = NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").ok();
                } else if let Some(value) = part.strip_prefix("dissolve_delay_years:") {
                    dissolve_delay_years = value.trim().parse().ok();
                }
            }

            // If we successfully parsed all required fields, add to metadata map
            if let (Some(id), Some(date), Some(delay)) = (neuron_id, created_date, dissolve_delay_years) {
                metadata_map.insert(id, NeuronMetadata {
                    created_date: date,
                    dissolve_delay_years: delay,
                });
            }
        }

        Ok(metadata_map)
    }

    /// Calculate age_seconds from age_bonus_multiplier
    /// ICP formula: age_bonus = 1.0 + (min(age, 4_years) / 4_years) * 0.25
    /// Reverse: age_seconds = ((age_bonus - 1.0) / 0.25) * 4_years
    /// Note: This gives the effective age capped at 4 years
    fn calculate_age_from_bonus(&self, age_bonus_multiplier: f64) -> u64 {
        // 4 years on a 365.25-day basis, matching BonusMultiplier::from_age_seconds, which
        // divides by 365.25 * 86400. The previous value (126_144_000 = 4 * 365 * 86400) used a
        // 365-day year, so inverting a bonus and re-deriving it scaled the bonus portion by
        // 365/365.25 = 0.9993155373032172 — an imported 1.25 came back as 1.2498288843258043,
        // and the maximum age bonus could not be represented at all.
        const FOUR_YEARS_SECONDS: f64 = 126_230_400.0; // 4 * 365.25 * 86400

        if age_bonus_multiplier <= 1.0 {
            return 0;
        }

        // Clamp to valid range [1.0, 1.25]
        let clamped_bonus = age_bonus_multiplier.min(1.25);

        // Reverse the formula: age = ((bonus - 1.0) / 0.25) * 4_years
        let normalized = (clamped_bonus - 1.0) / 0.25;
        let age_seconds = normalized * FOUR_YEARS_SECONDS;

        age_seconds.round() as u64
    }

    /// Parse a single CSV record into a CsvRecord
    fn parse_record(
        &self,
        headers: &StringRecord,
        record: &StringRecord,
        row_num: usize,
        metadata: &HashMap<u64, NeuronMetadata>,
    ) -> Result<CsvRecord, Box<dyn std::error::Error>> {
        // Helper to get field value by column name
        let get_field = |name: &str| -> Result<&str, Box<dyn std::error::Error>> {
            let pos = headers.iter().position(|h| h == name)
                .ok_or_else(|| format!("Column '{}' not found", name))?;
            record.get(pos)
                .ok_or_else(|| format!("Row {}: Missing value for column '{}'", row_num, name).into())
        };

        // Parse neuron_id
        let neuron_id_str = get_field("neuron_id")?;
        let neuron_id = neuron_id_str.parse::<u64>()
            .map_err(|e| format!(
                "Row {}: Invalid neuron_id '{}'. Expected positive integer. Error: {}",
                row_num, neuron_id_str, e
            ))?;

        // Parse snapshot_date
        let date_str = get_field("snapshot_date")?;
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .map_err(|e| format!(
                "Row {}: Invalid snapshot_date '{}'. Expected format: YYYY-MM-DD. Error: {}",
                row_num, date_str, e
            ))?;

        // Parse stake_e8s
        // Support both decimal format (e.g., 1000.00000000 ICP from Excel) and integer format (e8s)
        // Decimal values are multiplied by 10^8 to convert to e8s
        let stake_str = get_field("stake_e8s")?;
        let stake_e8s = if stake_str.contains('.') {
            // Decimal format (ICP) - multiply by 10^8 to get e8s
            let decimal_value = stake_str.parse::<f64>()
                .map_err(|e| format!(
                    "Row {}: Invalid stake_e8s '{}'. Expected decimal number. Error: {}",
                    row_num, stake_str, e
                ))?;
            (decimal_value * 100_000_000.0).round() as u64
        } else {
            // Integer format (e8s) - use as-is
            stake_str.parse::<u64>()
                .map_err(|e| format!(
                    "Row {}: Invalid stake_e8s '{}'. Expected positive integer. Error: {}",
                    row_num, stake_str, e
                ))?
        };

        // Parse staked_maturity_e8s
        // Support both decimal format (ICP) and integer format (e8s)
        let staked_maturity_str = get_field("staked_maturity_e8s")?;
        let staked_maturity_e8s = if staked_maturity_str.contains('.') {
            // Decimal format (ICP) - multiply by 10^8 to get e8s
            let decimal_value = staked_maturity_str.parse::<f64>()
                .map_err(|e| format!(
                    "Row {}: Invalid staked_maturity_e8s '{}'. Expected decimal number. Error: {}",
                    row_num, staked_maturity_str, e
                ))?;
            (decimal_value * 100_000_000.0).round() as u64
        } else {
            // Integer format (e8s) - use as-is
            staked_maturity_str.parse::<u64>()
                .map_err(|e| format!(
                    "Row {}: Invalid staked_maturity_e8s '{}'. Expected positive integer. Error: {}",
                    row_num, staked_maturity_str, e
                ))?
        };

        // Parse available_maturity_e8s (optional - defaults to 0 if not present)
        // Support both decimal format (ICP) and integer format (e8s)
        let maturity_e8s = if let Ok(maturity_str) = get_field("available_maturity_e8s") {
            if maturity_str.contains('.') {
                // Decimal format (ICP) - multiply by 10^8 to get e8s
                let decimal_value = maturity_str.parse::<f64>()
                    .map_err(|e| format!(
                        "Row {}: Invalid available_maturity_e8s '{}'. Expected decimal number. Error: {}",
                        row_num, maturity_str, e
                    ))?;
                (decimal_value * 100_000_000.0).round() as u64
            } else {
                // Integer format (e8s) - use as-is
                maturity_str.parse::<u64>()
                    .map_err(|e| format!(
                        "Row {}: Invalid available_maturity_e8s '{}'. Expected positive integer. Error: {}",
                        row_num, maturity_str, e
                    ))?
            }
        } else {
            // Default to 0 if field not present
            0
        };

        // Parse voting_power
        // Support both decimal format (e.g., 3000.00000000 from Excel) and integer format
        // Decimal values are multiplied by 10^8 to convert to storage format
        let voting_power_str = get_field("voting_power")?;
        let voting_power = if voting_power_str.contains('.') {
            // Decimal format - multiply by 10^8
            let decimal_value = voting_power_str.parse::<f64>()
                .map_err(|e| format!(
                    "Row {}: Invalid voting_power '{}'. Expected decimal number. Error: {}",
                    row_num, voting_power_str, e
                ))?;
            (decimal_value * 100_000_000.0).round() as u64
        } else {
            // Integer format - use as-is
            voting_power_str.parse::<u64>()
                .map_err(|e| format!(
                    "Row {}: Invalid voting_power '{}'. Expected positive integer. Error: {}",
                    row_num, voting_power_str, e
                ))?
        };

        // Determine if this is simplified format (has age_bonus_multiplier) or traditional format
        let is_simplified = headers.iter().any(|h| h == "age_bonus_multiplier");

        let (dissolve_delay_seconds, age_seconds, created_timestamp) = if is_simplified {
            // Simplified format: Use metadata and calculate from age_bonus_multiplier
            let meta = metadata.get(&neuron_id)
                .ok_or_else(|| format!(
                    "Row {}: No metadata found for neuron_id {}. \
                    Expected metadata comment like: # neuron_id: {}, created_date: YYYY-MM-DD, dissolve_delay_years: 8",
                    row_num, neuron_id, neuron_id
                ))?;

            // Parse age_bonus_multiplier
            let age_bonus_str = get_field("age_bonus_multiplier")?;
            let age_bonus = age_bonus_str.parse::<f64>()
                .map_err(|e| format!(
                    "Row {}: Invalid age_bonus_multiplier '{}'. Expected decimal number. Error: {}",
                    row_num, age_bonus_str, e
                ))?;

            // Calculate fields from metadata
            // 365.25-day years, matching BonusMultiplier::from_dissolve_seconds, which divides
            // by 365.25 * 86400. The previous value (31_536_000 = 365 * 86400) meant a metadata
            // "dissolve_delay_years: 8" round-tripped to a 1.999315537303217 bonus instead of
            // 2.0 — the same 365/365.25 defect as the age inversion above.
            let dissolve_delay = meta.dissolve_delay_years * 31_557_600; // years to seconds (365.25 * 86400)
            let age = self.calculate_age_from_bonus(age_bonus);
            let created_ts = meta.created_date.and_hms_opt(0, 0, 0)
                .ok_or_else(|| format!("Row {}: Invalid created_date in metadata", row_num))?
                .and_utc()
                .timestamp() as u64;

            (dissolve_delay, age, created_ts)
        } else {
            // Traditional format: Parse from columns
            let dissolve_delay_str = get_field("dissolve_delay_seconds")?;
            let dissolve_delay = dissolve_delay_str.parse::<u64>()
                .map_err(|e| format!(
                    "Row {}: Invalid dissolve_delay_seconds '{}'. Expected positive integer. Error: {}",
                    row_num, dissolve_delay_str, e
                ))?;

            let age_str = get_field("age_seconds")?;
            let age = age_str.parse::<u64>()
                .map_err(|e| format!(
                    "Row {}: Invalid age_seconds '{}'. Expected positive integer. Error: {}",
                    row_num, age_str, e
                ))?;

            let created_str = get_field("created_timestamp_seconds")?;
            let created_ts = created_str.parse::<u64>()
                .map_err(|e| format!(
                    "Row {}: Invalid created_timestamp_seconds '{}'. Expected positive integer. Error: {}",
                    row_num, created_str, e
                ))?;

            (dissolve_delay, age, created_ts)
        };

        // Determine neuron state based on dissolve delay
        // Note: CSV doesn't have state field, so we infer it
        // If dissolve_delay > 0, assume Locked (this is simplified logic)
        let state = if dissolve_delay_seconds > 0 {
            NeuronState::Locked
        } else {
            NeuronState::Dissolved
        };

        // Auto-stake is enabled if staked_maturity > 0 (inference)
        let auto_stake_enabled = staked_maturity_e8s > 0;

        // Observation time, when the file carries it.
        //
        // Export format 1.1 writes `retrieved_timestamp_seconds`; 1.0 did not, and neither do
        // hand-written or spreadsheet-derived files. Where it is absent, `Neuron::new` stamps
        // the import time — which is the honest answer for a row whose observation time was
        // never recorded, and is what marks a bulk import as a bulk import. Where it is
        // present, it must survive: re-importing an export is the backup-and-restore path the
        // export's own summary recommends, and losing provenance across it would silently
        // relabel years of automated collection as having been observed today.
        let retrieved_timestamp = headers
            .iter()
            .position(|h| h == "retrieved_timestamp_seconds")
            .and_then(|idx| record.get(idx))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                value.parse::<u64>().map_err(|e| format!(
                    "Row {}: Invalid retrieved_timestamp_seconds '{}'. Expected positive integer. Error: {}",
                    row_num, value, e
                ))
            })
            .transpose()?;

        // Create Neuron domain object
        let neuron = match retrieved_timestamp {
            Some(observed_at) => Neuron::from_snapshot(
                NeuronId::new(neuron_id),
                IcpAmount::from_e8s(stake_e8s),
                IcpAmount::from_e8s(maturity_e8s),
                IcpAmount::from_e8s(staked_maturity_e8s),
                voting_power,
                age_seconds,
                dissolve_delay_seconds,
                state,
                auto_stake_enabled,
                created_timestamp,
                observed_at,
            ),
            None => Neuron::new(
                NeuronId::new(neuron_id),
                IcpAmount::from_e8s(stake_e8s),
                IcpAmount::from_e8s(maturity_e8s),
                IcpAmount::from_e8s(staked_maturity_e8s),
                voting_power,
                age_seconds,
                dissolve_delay_seconds,
                state,
                auto_stake_enabled,
                created_timestamp,
            ),
        };

        Ok(CsvRecord {
            neuron_id: NeuronId::new(neuron_id),
            date,
            neuron,
        })
    }
}

impl Default for CsvParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_valid_csv_traditional_format() {
        let csv_content = "\
neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,available_maturity_e8s,voting_power,dissolve_delay_seconds,age_seconds,created_timestamp_seconds
10000000000000000001,2022-10-20,50000000000,150000000,0,900,252288000,12084595,1600000000
100000000000000004,2022-10-20,40000000000,140000000,0,800,252288000,12084595,1600000000
";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let parser = CsvParser::new();
        let records = parser.parse_file(temp_file.path()).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].neuron_id.value(), 10000000000000000001);
        assert_eq!(records[0].date.to_string(), "2022-10-20");
        assert_eq!(records[0].neuron.stake().e8s(), 50000000000);

        assert_eq!(records[1].neuron_id.value(), 100000000000000004);
    }

    #[test]
    fn test_parse_missing_header() {
        let csv_content = "\
neuron_id,snapshot_date,stake_e8s
10000000000000000001,2022-10-20,50000000000
";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let parser = CsvParser::new();
        let result = parser.parse_file(temp_file.path());

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        // Should fail validation because format is not recognized (missing both age_seconds and age_bonus_multiplier)
        assert!(error_msg.contains("CSV format not recognized") || error_msg.contains("Missing required column"));
    }

    #[test]
    fn test_parse_invalid_date_format() {
        let csv_content = "\
neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,available_maturity_e8s,voting_power,dissolve_delay_seconds,age_seconds,created_timestamp_seconds
10000000000000000001,10/20/2022,50000000000,150000000,0,900,252288000,12084595,1600000000
";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let parser = CsvParser::new();
        let result = parser.parse_file(temp_file.path());

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Invalid snapshot_date"));
        assert!(error_msg.contains("YYYY-MM-DD"));
    }

    #[test]
    fn test_parse_invalid_integer() {
        let csv_content = "\
neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,available_maturity_e8s,voting_power,dissolve_delay_seconds,age_seconds,created_timestamp_seconds
10000000000000000001,2022-10-20,not_a_number,150000000,0,900,252288000,12084595,1600000000
";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let parser = CsvParser::new();
        let result = parser.parse_file(temp_file.path());

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Invalid stake_e8s"));
    }

    #[test]
    fn test_parse_decimal_voting_power() {
        // Test that decimal voting_power (e.g., from Excel) is correctly converted
        // 3000.00000000 should become 300000000000 (multiplied by 10^8)
        let csv_content = "\
neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,available_maturity_e8s,voting_power,dissolve_delay_seconds,age_seconds,created_timestamp_seconds
10000000000000000001,2022-10-20,50000000000,150000000,0,3000.00000000,252288000,12084595,1600000000
";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let parser = CsvParser::new();
        let records = parser.parse_file(temp_file.path()).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].neuron.voting_power(), 300000000000);
    }

    #[test]
    fn test_parse_integer_voting_power() {
        // Test that integer voting_power still works (backward compatibility)
        let csv_content = "\
neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,available_maturity_e8s,voting_power,dissolve_delay_seconds,age_seconds,created_timestamp_seconds
10000000000000000001,2022-10-20,50000000000,150000000,0,300000000000,252288000,12084595,1600000000
";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let parser = CsvParser::new();
        let records = parser.parse_file(temp_file.path()).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].neuron.voting_power(), 300000000000);
    }

    #[test]
    fn test_parse_decimal_stake_and_maturity() {
        // Test that decimal stake and maturity (e.g., from Excel in ICP format) are correctly converted
        // 1000.00000000 ICP should become 100000000000 e8s (multiplied by 10^8)
        // 250.00000000 ICP should become 25000000000 e8s
        let csv_content = "\
neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,available_maturity_e8s,voting_power,dissolve_delay_seconds,age_seconds,created_timestamp_seconds
10000000000000000001,2022-10-20,1000.00000000,250.00000000,5.25,3000.00000000,252288000,12084595,1600000000
";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let parser = CsvParser::new();
        let records = parser.parse_file(temp_file.path()).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].neuron.stake().e8s(), 100000000000);
        assert_eq!(records[0].neuron.staked_maturity().e8s(), 25000000000);
        assert_eq!(records[0].neuron.maturity().e8s(), 525000000); // 5.25 ICP
        assert_eq!(records[0].neuron.voting_power(), 300000000000);
    }

    #[test]
    fn test_parse_mixed_decimal_and_integer_fields() {
        // Test mixing decimal and integer formats in same CSV
        let csv_content = "\
neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,available_maturity_e8s,voting_power,dissolve_delay_seconds,age_seconds,created_timestamp_seconds
10000000000000000001,2022-10-20,100000000000,250.00000000,0,3000.00000000,252288000,12084595,1600000000
";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let parser = CsvParser::new();
        let records = parser.parse_file(temp_file.path()).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].neuron.stake().e8s(), 100000000000); // Integer e8s format
        assert_eq!(records[0].neuron.staked_maturity().e8s(), 25000000000); // Decimal ICP format
        assert_eq!(records[0].neuron.voting_power(), 300000000000); // Decimal format
    }

    #[test]
    fn test_parse_simplified_format_with_metadata() {
        // Test simplified format with metadata in comments
        let csv_content = "\
# Neuron Configuration Metadata
# neuron_id: 10000000000000000001, created_date: 2021-05-17, dissolve_delay_years: 8
# neuron_id: 100000000000000004, created_date: 2021-05-18, dissolve_delay_years: 8
#
neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,voting_power,age_bonus_multiplier
10000000000000000001,2022-10-20,500.00000000,1.50000000,900.00000000,1.02395
100000000000000004,2022-10-20,400.00000000,1.40000000,800.00000000,1.02408
";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let parser = CsvParser::new();
        let records = parser.parse_file(temp_file.path()).unwrap();

        assert_eq!(records.len(), 2);

        // Check first record
        assert_eq!(records[0].neuron_id.value(), 10000000000000000001);
        assert_eq!(records[0].date.to_string(), "2022-10-20");
        assert_eq!(records[0].neuron.stake().e8s(), 50000000000); // 500.00000000 * 10^8
        assert_eq!(records[0].neuron.staked_maturity().e8s(), 150000000); // 1.50000000 * 10^8
        assert_eq!(records[0].neuron.voting_power(), 90000000000); // 900.00000000 * 10^8
        // 8 years on a 365.25-day basis. Previously asserted 2920 (a 365-day year), which
        // restated the implementation's constant rather than testing it; the round-trip
        // property is asserted in dissolve_bonus_inverts_exactly_against_from_dissolve_seconds.
        assert_eq!(records[0].neuron.dissolve_delay_days(), 2922);
        assert_eq!(records[0].neuron.maturity().e8s(), 0); // Default when not provided

        // Check that age was calculated from age_bonus_multiplier (1.02395)
        // age_bonus = 1.02395 means age ~= 0.02395 / 0.25 * 4_years ~= 12,084,672 seconds ~= 139.87 days
        let age_days = records[0].neuron.age_days();
        assert!(age_days >= 139 && age_days <= 140, "Age should be around 140 days, got {}", age_days);

        // Check second record
        assert_eq!(records[1].neuron_id.value(), 100000000000000004);
    }

    #[test]
    fn test_calculate_age_from_bonus() {
        let parser = CsvParser::new();

        // Test age_bonus = 1.0 (no age)
        assert_eq!(parser.calculate_age_from_bonus(1.0), 0);

        // Years are 365.25 days, matching BonusMultiplier::from_age_seconds. These constants
        // previously used a 365-day year (63_072_000 / 126_144_000), which is the defect this
        // function carried — the test encoded it rather than catching it.
        let two_years = 63_115_200;   // 2 * 365.25 * 86400
        let four_years = 126_230_400; // 4 * 365.25 * 86400

        // Test age_bonus = 1.125 (2 years)
        // 1.125 = 1.0 + (2_years / 4_years) * 0.25
        let age = parser.calculate_age_from_bonus(1.125);
        assert!((age as i64 - two_years as i64).abs() < 100, "Expected ~2 years, got {}", age);

        // Test age_bonus = 1.25 (4 years - maximum)
        let age = parser.calculate_age_from_bonus(1.25);
        assert!((age as i64 - four_years as i64).abs() < 100, "Expected ~4 years, got {}", age);

        // Test age_bonus > 1.25 (should clamp to 4 years)
        let age = parser.calculate_age_from_bonus(1.5);
        assert!((age as i64 - four_years as i64).abs() < 100, "Should clamp to 4 years, got {}", age);
    }

    /// The provenance marker must survive an export/import round-trip.
    ///
    /// The export omitted `retrieved_timestamp_seconds` entirely, so re-importing a backup —
    /// which the export's own "next steps" recommends — stamped every row with the import
    /// time via `Neuron::new`. Years of automated collection came back indistinguishable from
    /// a bulk import, destroying the one column that tells them apart.
    #[test]
    fn retrieved_timestamp_survives_a_csv_round_trip() {
        let observed_at: u64 = 1_767_816_300; // 2026-01-07 18:45:00 UTC
        let csv_content = format!(
            "neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,available_maturity_e8s,voting_power,dissolve_delay_seconds,age_seconds,created_timestamp_seconds,retrieved_timestamp_seconds\n\
10000000000000000001,2026-01-07,100000000000,25000000000,0,300000000000,63072000,85622400,1600000000,{}\n",
            observed_at
        );

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let records = CsvParser::new().parse_file(temp_file.path()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].neuron.retrieved_at().timestamp(),
            observed_at as i64,
            "the observation time must come from the file, not from now()"
        );
    }

    /// A file without the column is still importable, and is stamped with the import time —
    /// the honest answer for a row whose observation time was never recorded. This keeps
    /// format 1.0 exports and hand-written spreadsheets working.
    #[test]
    fn a_csv_without_the_provenance_column_still_imports() {
        let before = chrono::Utc::now().timestamp();
        let csv_content = "\
neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,available_maturity_e8s,voting_power,dissolve_delay_seconds,age_seconds,created_timestamp_seconds
10000000000000000001,2026-01-07,100000000000,25000000000,0,300000000000,63072000,85622400,1600000000
";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let records = CsvParser::new().parse_file(temp_file.path()).unwrap();
        assert_eq!(records.len(), 1);
        assert!(
            records[0].neuron.retrieved_at().timestamp() >= before,
            "absent column falls back to the import time"
        );
    }

    /// A metadata "dissolve_delay_years: N" must survive conversion to seconds and back
    /// through BonusMultiplier::from_dissolve_seconds. The 365-day constant broke this: the
    /// ceiling was unreachable because the recovered year count fell fractionally short.
    /// Asserts the property, not the constant — no 365-day value can satisfy it.
    ///
    /// Updated 2026-08-06 for the Mission 70 curve: quadratic to 3.0x at two years, replacing
    /// linear to 2.0x at eight. The round-trip property is unchanged; only the curve moved.
    #[test]
    fn dissolve_bonus_inverts_exactly_against_from_dissolve_seconds() {
        use crate::domain::BonusMultiplier;

        // Mirrors the conversion in parse_record for the simplified format.
        let years_to_seconds = |years: u64| years * 31_557_600;

        // Below the two-year cap the bonus is quadratic in the fraction of the maximum, so a
        // whole number of years must recover exactly rather than a hair under.
        for years in [1_u64] {
            let recovered = BonusMultiplier::from_dissolve_seconds(years_to_seconds(years)).value();
            let fraction = years as f64 / 2.0;
            let expected = 1.0 + 2.0 * fraction * fraction;
            assert!(
                (recovered - expected).abs() < 1e-9,
                "{} years -> {} seconds -> bonus {} (expected {})",
                years, years_to_seconds(years), recovered, expected
            );
        }

        // The two-year maximum specifically, at full precision.
        let max = BonusMultiplier::from_dissolve_seconds(years_to_seconds(2)).value();
        assert_eq!(max, 3.0, "maximum dissolve bonus must be exactly representable");

        // Delays beyond the maximum stay pinned at the cap rather than running away — the
        // historical CSV carries "dissolve_delay_years: 8" from the pre-Mission-70 protocol.
        let legacy_eight_year = BonusMultiplier::from_dissolve_seconds(years_to_seconds(8)).value();
        assert_eq!(legacy_eight_year, 3.0, "8 years is beyond the 2-year cap, not 4x the bonus");

        // 730 days, since 2 x 365.25 = 730.5 truncates.
        assert_eq!(years_to_seconds(2) / 86400, 730);
    }

    /// The inversion must round-trip exactly against the forward function, not approximately.
    /// This is the property the 365-day constant broke: an imported 1.25 came back as
    /// 1.2498288843258043, so the maximum age bonus was unrepresentable.
    #[test]
    fn age_bonus_inverts_exactly_against_from_age_seconds() {
        use crate::domain::BonusMultiplier;
        let parser = CsvParser::new();

        for bonus in [1.25_f64, 1.125, 1.0625] {
            let age_seconds = parser.calculate_age_from_bonus(bonus);
            let recovered = BonusMultiplier::from_age_seconds(age_seconds).value();
            assert!(
                (recovered - bonus).abs() < 1e-9,
                "bonus {} inverted to {} seconds and came back as {} (delta {:e})",
                bonus, age_seconds, recovered, recovered - bonus
            );
        }

        // The maximum specifically, at full precision.
        let max = BonusMultiplier::from_age_seconds(parser.calculate_age_from_bonus(1.25)).value();
        assert_eq!(max, 1.25, "maximum age bonus must be exactly representable");
    }

    #[test]
    fn test_parse_metadata_from_comments() {
        let csv_content = "\
# Neuron Configuration Metadata
# neuron_id: 10000000000000000001, created_date: 2021-05-17, dissolve_delay_years: 8
# neuron_id: 100000000000000004, created_date: 2021-05-18, dissolve_delay_years: 8
# This is just a regular comment, should be ignored
#
neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,voting_power,age_bonus_multiplier
";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let parser = CsvParser::new();
        let metadata = parser.parse_metadata(temp_file.path()).unwrap();

        assert_eq!(metadata.len(), 2);

        let meta1 = metadata.get(&10000000000000000001).unwrap();
        assert_eq!(meta1.created_date.to_string(), "2021-05-17");
        assert_eq!(meta1.dissolve_delay_years, 8);

        let meta2 = metadata.get(&100000000000000004).unwrap();
        assert_eq!(meta2.created_date.to_string(), "2021-05-18");
        assert_eq!(meta2.dissolve_delay_years, 8);
    }

    #[test]
    fn test_simplified_format_missing_metadata() {
        // Test that simplified format without metadata gives helpful error
        let csv_content = "\
neuron_id,snapshot_date,stake_e8s,staked_maturity_e8s,voting_power,age_bonus_multiplier
10000000000000000001,2022-10-20,500.00000000,1.50000000,900.00000000,1.02395
";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(csv_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let parser = CsvParser::new();
        let result = parser.parse_file(temp_file.path());

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("No metadata found for neuron_id"));
        assert!(error_msg.contains("Expected metadata comment"));
    }
}
