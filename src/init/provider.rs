//! Choosing a provider: a preset with connection defaults, or a custom
//! OpenAI-compatible endpoint.

use std::{
    collections::BTreeMap,
    io::{BufRead, Write},
};

use super::{Dialogue, InitError, Selection};
use crate::{
    config::{DEFAULT_TIMEOUT_MS, ProviderConfig},
    validate::{self, Value},
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

const LOCAL_LABEL: &str = "Local model server";
const CUSTOM_LABEL: &str = "Custom OpenAI-compatible endpoint";
/// The first request to a local server often waits for the model to load.
const LOCAL_TIMEOUT_MS: u64 = 120_000;

/// A model server the user runs, reached with no credential.
pub(super) struct LocalPreset {
    menu_label: &'static str,
    name: &'static str,
    base_url: &'static str,
    /// How the server is usually started, shown when it cannot be reached.
    start_hint: &'static str,
}

pub(super) const LOCAL_PRESETS: [LocalPreset; 3] = [
    LocalPreset {
        menu_label: "Ollama",
        name: "ollama",
        base_url: "http://localhost:11434/v1",
        start_hint: "Ollama is usually started with `ollama serve`",
    },
    LocalPreset {
        menu_label: "LM Studio",
        name: "lmstudio",
        base_url: "http://localhost:1234/v1",
        start_hint: "LM Studio is usually started with `lms server start`",
    },
    LocalPreset {
        menu_label: "llama.cpp server",
        name: "llamacpp",
        base_url: "http://localhost:8080/v1",
        start_hint: "llama.cpp is usually started with `llama-server -m <model.gguf>`",
    },
];

type Providers = BTreeMap<String, ProviderConfig>;

impl<R: BufRead, W: Write> Dialogue<'_, R, W> {
    /// Chooses a provider and returns its name, unique among `existing`.
    pub(super) fn provider(&mut self, existing: &Providers) -> Result<Selection, InitError> {
        let mut labels: Vec<String> = PRESETS
            .iter()
            .map(|preset| preset.menu_label.to_string())
            .collect();
        labels.push(LOCAL_LABEL.to_string());
        labels.push(CUSTOM_LABEL.to_string());
        let choice = self.choose("Select a provider", &labels)?;
        match PRESETS.get(choice) {
            Some(preset) => self.preset_provider(preset, existing),
            None if choice == PRESETS.len() => self.local_provider(existing),
            None => self.custom_provider(existing),
        }
    }

    /// The position of the chosen label, from a menu or a numbered list.
    pub(super) fn choose(&mut self, title: &str, labels: &[String]) -> Result<usize, InitError> {
        if !self.console.menus {
            return self.numbered(title, labels);
        }
        let heading = format!("{title} (arrow keys, Enter; Esc cancels):");
        crate::terminal::select(&heading, labels)?.ok_or(InitError::Cancelled)
    }

    fn local_provider(&mut self, existing: &Providers) -> Result<Selection, InitError> {
        let labels: Vec<String> = LOCAL_PRESETS
            .iter()
            .map(|preset| preset.menu_label.to_string())
            .collect();
        let preset = &LOCAL_PRESETS[self.choose("Select a local model server", &labels)?];
        self.say(&format!(
            "\nProvider: {} (kind {COMPATIBLE}, no credential)",
            preset.menu_label
        ))?;
        let prompt = format!("Endpoint base URL [{}]: ", preset.base_url);
        let base_url = self.endpoint_or(&prompt, preset.base_url)?;
        let name = self.unused_name(preset.name, existing)?;
        let provider = ProviderConfig {
            kind: COMPATIBLE.to_string(),
            base_url,
            api_key_env: None,
            timeout_ms: LOCAL_TIMEOUT_MS,
        };
        Ok(Selection {
            name,
            provider,
            start_hint: Some(preset.start_hint),
        })
    }

    /// A valid endpoint typed at `prompt`; an empty answer takes `default`.
    pub(super) fn endpoint_or(&mut self, prompt: &str, default: &str) -> Result<String, InitError> {
        loop {
            let answer = self.ask(prompt)?;
            if answer.is_empty() {
                return Ok(default.to_string());
            }
            match validate::endpoint(Value(&answer)) {
                Ok(()) => return Ok(answer),
                Err(rule) => self.say(&format!("That value {rule}."))?,
            }
        }
    }

    /// `preferred` when no provider has that name, otherwise a name asked for.
    fn unused_name(&mut self, preferred: &str, existing: &Providers) -> Result<String, InitError> {
        if !existing.contains_key(preferred) {
            return Ok(preferred.to_string());
        }
        self.say(&format!(
            "A provider named '{preferred}' is already configured."
        ))?;
        self.provider_name(existing)
    }

    fn preset_provider(
        &mut self,
        preset: &Preset,
        existing: &Providers,
    ) -> Result<Selection, InitError> {
        self.say(&format!(
            "\nProvider: {} (kind {})",
            preset.menu_label, preset.kind
        ))?;
        self.say(&format!("Endpoint: {}", preset.base_url))?;
        self.say(&format!("Credential variable: {}", preset.api_key_env))?;
        let name = self.unused_name(preset.name, existing)?;
        let provider = ProviderConfig {
            kind: preset.kind.to_string(),
            base_url: preset.base_url.to_string(),
            api_key_env: Some(preset.api_key_env.to_string()),
            timeout_ms: DEFAULT_TIMEOUT_MS,
        };
        Ok(Selection {
            name,
            provider,
            start_hint: None,
        })
    }

    fn custom_provider(&mut self, existing: &Providers) -> Result<Selection, InitError> {
        self.say("\nCustom OpenAI-compatible endpoint (kind openai-compatible).")?;
        let provider_name = self.provider_name(existing)?;
        let base_url = self.required(
            "Endpoint base URL (http:// or https://): ",
            validate::endpoint,
        )?;
        let api_key_env = self.custom_credential_variable()?;
        let provider = ProviderConfig {
            kind: COMPATIBLE.to_string(),
            base_url,
            api_key_env,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        };
        Ok(Selection {
            name: provider_name,
            provider,
            start_hint: None,
        })
    }

    /// The credential variable for a custom endpoint; an empty answer means none.
    fn custom_credential_variable(&mut self) -> Result<Option<String>, InitError> {
        let prompt = "Credential environment variable name (empty means no credential): ";
        loop {
            let answer = self.ask(prompt)?;
            match validate::env_var_name(Value(&answer)) {
                _ if answer.is_empty() => return Ok(None),
                Ok(()) => return Ok(Some(answer)),
                Err(rule) => self.say(&format!("That value {rule}."))?,
            }
        }
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
