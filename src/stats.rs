use std::{fmt, time::Duration};

use crate::provider::Usage;

pub struct Statistics {
    pub model: String,
    pub wall: Duration,
    pub api: Duration,
    pub ttft: Duration,
    pub usage: Option<Usage>,
}

impl fmt::Display for Statistics {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (input, output) = token_counts(self.usage);
        write!(
            formatter,
            "{} · {:.1}s wall · {:.1}s api · {:.1}s to first token · {input} in / {output} out",
            self.model,
            self.wall.as_secs_f64(),
            self.api.as_secs_f64(),
            self.ttft.as_secs_f64()
        )
    }
}

fn token_counts(usage: Option<Usage>) -> (String, String) {
    usage.map_or_else(
        || ("?".to_string(), "?".to_string()),
        |usage| (usage.input.to_string(), usage.output.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_reported_usage_and_tenths() {
        let statistics = Statistics {
            model: "model".to_string(),
            wall: Duration::from_millis(1250),
            api: Duration::from_millis(240),
            ttft: Duration::from_millis(149),
            usage: Some(Usage {
                input: 12,
                output: 3,
            }),
        };
        assert_eq!(
            statistics.to_string(),
            "model · 1.2s wall · 0.2s api · 0.1s to first token · 12 in / 3 out"
        );
    }

    #[test]
    fn formats_missing_usage_as_questions() {
        assert_eq!(token_counts(None), ("?".to_string(), "?".to_string()));
    }
}
