//! Choosing a provider: a preset with connection defaults, or a custom
//! OpenAI-compatible endpoint.

use std::{
    collections::BTreeMap,
    io::{BufRead, Write},
};

use super::{Dialogue, InitError};
use crate::{
    config::{DEFAULT_TIMEOUT_MS, ProviderConfig},
    validate,
};

pub(super) struct Preset {
    menu_label: &'static str,
    kind: &'static str,
    name: &'static str,
    base_url: &'static str,
    api_key_env: &'static str,
}

const fn preset(
    menu_label: &'static str,
    kind: &'static str,
    base_url: &'static str,
    api_key_env: &'static str,
) -> Preset {
    Preset {
        menu_label,
        kind,
        name: kind,
        base_url,
        api_key_env,
    }
}

const COMPATIBLE: &str = validate::PROVIDER_KIND;

pub(super) const PRESETS: [Preset; 7] = [
    preset(
        "OpenAI",
        "openai",
        "https://api.openai.com/v1",
        "OPENAI_API_KEY",
    ),
    preset(
        "Anthropic",
        "anthropic",
        "https://api.anthropic.com",
        "ANTHROPIC_API_KEY",
    ),
    preset(
        "Gemini",
        "gemini",
        "https://generativelanguage.googleapis.com",
        "GEMINI_API_KEY",
    ),
    preset(
        "OpenRouter",
        "openrouter",
        "https://openrouter.ai/api/v1",
        "OPENROUTER_API_KEY",
    ),
    Preset {
        name: "groq",
        ..preset(
            "Groq",
            COMPATIBLE,
            "https://api.groq.com/openai/v1",
            "GROQ_API_KEY",
        )
    },
    Preset {
        name: "cerebras",
        ..preset(
            "Cerebras",
            COMPATIBLE,
            "https://api.cerebras.ai/v1",
            "CEREBRAS_API_KEY",
        )
    },
    Preset {
        name: "xai",
        ..preset("xAI", COMPATIBLE, "https://api.x.ai/v1", "XAI_API_KEY")
    },
];

const CUSTOM_LABEL: &str = "Custom OpenAI-compatible endpoint";

type Providers = BTreeMap<String, ProviderConfig>;

impl<R: BufRead, W: Write> Dialogue<'_, R, W> {
    /// Chooses a provider and returns its name, unique among `existing`.
    pub(super) fn provider(
        &mut self,
        existing: &Providers,
    ) -> Result<(String, ProviderConfig), InitError> {
        let mut labels: Vec<String> = PRESETS
            .iter()
            .map(|preset| preset.menu_label.to_string())
            .collect();
        labels.push(CUSTOM_LABEL.to_string());
        let choice = if self.console.menus {
            crate::terminal::select(
                "Select a provider (arrow keys, Enter; Esc cancels):",
                &labels,
            )?
            .ok_or(InitError::Cancelled)?
        } else {
            self.numbered("Select a provider", &labels)?
        };
        match PRESETS.get(choice) {
            Some(preset) => self.preset_provider(preset, existing),
            None => self.custom_provider(existing),
        }
    }

    fn preset_provider(
        &mut self,
        preset: &Preset,
        existing: &Providers,
    ) -> Result<(String, ProviderConfig), InitError> {
        self.say(&format!(
            "\nProvider: {} (kind {})",
            preset.menu_label, preset.kind
        ))?;
        self.say(&format!("Endpoint: {}", preset.base_url))?;
        self.say(&format!("Credential variable: {}", preset.api_key_env))?;
        let name = if existing.contains_key(preset.name) {
            self.say(&format!(
                "A provider named '{}' is already configured.",
                preset.name
            ))?;
            self.provider_name(existing)?
        } else {
            preset.name.to_string()
        };
        Ok((
            name,
            ProviderConfig {
                kind: preset.kind.to_string(),
                base_url: preset.base_url.to_string(),
                api_key_env: preset.api_key_env.to_string(),
                timeout_ms: DEFAULT_TIMEOUT_MS,
            },
        ))
    }

    fn custom_provider(
        &mut self,
        existing: &Providers,
    ) -> Result<(String, ProviderConfig), InitError> {
        self.say("\nCustom OpenAI-compatible endpoint (kind openai-compatible).")?;
        let provider_name = self.provider_name(existing)?;
        let base_url = self.required(
            "Endpoint base URL (http:// or https://): ",
            validate::endpoint,
        )?;
        let api_key_env = self.required(
            "Credential environment variable name: ",
            validate::env_var_name,
        )?;
        Ok((
            provider_name,
            ProviderConfig {
                kind: COMPATIBLE.to_string(),
                base_url,
                api_key_env,
                timeout_ms: DEFAULT_TIMEOUT_MS,
            },
        ))
    }

    fn provider_name(&mut self, existing: &Providers) -> Result<String, InitError> {
        loop {
            let name = self.required(
                "Provider name (used in configuration tables): ",
                validate::non_empty,
            )?;
            if !existing.contains_key(&name) {
                return Ok(name);
            }
            self.say("That provider name is already used; choose another.")?;
        }
    }
}
