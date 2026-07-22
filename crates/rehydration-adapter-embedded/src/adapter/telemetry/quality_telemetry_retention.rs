use rehydration_domain::PortError;

/// Bounded-journal policy for local quality observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualityTelemetryRetention {
    max_observations: u64,
}

impl QualityTelemetryRetention {
    pub fn new(max_observations: u64) -> Result<Self, PortError> {
        if max_observations == 0 {
            return Err(PortError::Unavailable(
                "quality telemetry retention must keep at least one observation".to_string(),
            ));
        }
        Ok(Self { max_observations })
    }

    pub fn max_observations(&self) -> u64 {
        self.max_observations
    }

    pub fn excess(&self, total: u64) -> u64 {
        total.saturating_sub(self.max_observations)
    }
}

impl Default for QualityTelemetryRetention {
    fn default() -> Self {
        Self {
            max_observations: 100_000,
        }
    }
}
