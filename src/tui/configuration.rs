use super::input::Input;
use crate::config::{ModelConfigStore, NewModelProfile};
use std::time::Duration;

pub(super) const LABELS: [&str; 6] = [
    "Profile name",
    "Base URL (including /v1)",
    "Model ID",
    "API key",
    "Context tokens",
    "Output tokens",
];

pub(super) enum SetupPage {
    Profiles,
    Form,
    Workflows,
}

pub(super) struct Configuration {
    pub page: SetupPage,
    pub selected_profile: usize,
    pub selected_workflow: usize,
    pub fields: [Input; 6],
    pub focused: usize,
    pub error: Option<String>,
}

impl Configuration {
    pub fn new(store: &ModelConfigStore) -> Self {
        let selected_profile = store
            .active_config()
            .ok()
            .and_then(|active| {
                store
                    .configs()
                    .iter()
                    .position(|c| c.name() == active.name())
            })
            .unwrap_or(0);
        Self {
            page: if store.configs().is_empty() {
                SetupPage::Form
            } else {
                SetupPage::Profiles
            },
            selected_profile,
            selected_workflow: 0,
            fields: [
                "Default",
                "https://api.openai.com/v1",
                "",
                "",
                "128000",
                "4096",
            ]
            .map(Input::from),
            focused: 0,
            error: None,
        }
    }

    pub fn profile(&self) -> Result<NewModelProfile, String> {
        let value = |index: usize| self.fields[index].text.trim();
        if value(0).is_empty() || value(2).is_empty() || value(3).is_empty() {
            return Err("Profile name, model ID, and API key are required.".into());
        }
        let url = reqwest::Url::parse(value(1))
            .map_err(|_| "Enter a valid HTTP(S) base URL.".to_string())?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("Use an HTTP(S) base URL without credentials, query, or fragment.".into());
        }
        let tokens = |i: usize| {
            value(i)
                .parse::<usize>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| format!("{} must be a positive integer.", LABELS[i]))
        };
        let max_input_tokens = tokens(4)?;
        let max_output_tokens = tokens(5)?;
        if max_output_tokens > max_input_tokens {
            return Err("Output tokens cannot exceed the context limit.".into());
        }
        Ok(NewModelProfile {
            name: value(0).into(),
            host_url: value(1).trim_end_matches('/').into(),
            model_id: value(2).into(),
            api_key: value(3).into(),
            max_input_tokens,
            max_output_tokens,
            retry_amount: 2,
            max_backoff: Duration::from_secs(30),
        })
    }

    pub(super) fn save(&mut self, store: &mut ModelConfigStore) {
        let result = self.profile().and_then(|profile| {
            let name = profile.name.clone();
            store.create_profile(profile).map_err(|e| e.to_string())?;
            // Once persisted, never keep the plaintext credential in the form.
            self.fields[3] = Input::default();
            store.set_active(&name).map_err(|e| e.to_string())
        });
        match result {
            Ok(()) => {
                self.selected_profile = store.configs().len() - 1;
                self.page = SetupPage::Workflows;
            }
            Err(error) => self.error = Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{Event, KeyCode};
    use ratatui::{Terminal, backend::TestBackend};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TestStore {
        store: ModelConfigStore,
        root: PathBuf,
    }
    impl TestStore {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "cutlass-tui-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let store = ModelConfigStore::open(root.join("config"), root.join("keys")).unwrap();
            Self { store, root }
        }
    }
    impl Drop for TestStore {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn form() -> Configuration {
        Configuration {
            page: SetupPage::Form,
            selected_profile: 0,
            selected_workflow: 0,
            fields: [
                "local",
                "http://localhost:8080/v1",
                "model",
                "secret",
                "8192",
                "2048",
            ]
            .map(Input::from),
            focused: 0,
            error: None,
        }
    }

    #[test]
    fn validates_connection_before_persisting() {
        let mut config = form();
        assert_eq!(
            config.profile().unwrap().host_url,
            "http://localhost:8080/v1"
        );
        config.fields[1] = Input::from("file:///etc/passwd");
        assert!(config.profile().is_err());
        config.fields[1] = Input::from("https://example.com/v1?api_key=secret");
        assert!(config.profile().is_err());
        config.fields[1] = Input::from("https://example.com/v1");
        config.fields[5] = Input::from("9000");
        assert!(config.profile().is_err());
        config.fields[5] = Input::from("0");
        assert!(config.profile().is_err());
    }

    #[test]
    fn configuration_persists_encrypted_profile_before_workflow_selection() {
        let mut fixture = TestStore::new();
        assert!(matches!(
            Configuration::new(&fixture.store).page,
            SetupPage::Form
        ));
        let mut config = form();
        super::super::connection_configuration::handle_event(
            Event::Key(KeyCode::F(2).into()),
            &mut config,
            &mut fixture.store,
        );
        assert!(matches!(config.page, SetupPage::Workflows));
        assert!(config.fields[3].text.is_empty());
        assert!(config.error.is_none());
        let stored = fs::read_to_string(fixture.store.path()).unwrap();
        assert!(!stored.contains("secret"));
        assert_eq!(
            fixture
                .store
                .active_config()
                .unwrap()
                .decrypted_keys()
                .unwrap(),
            ["secret"]
        );
        super::super::workflow_list::handle_event(Event::Key(KeyCode::Down.into()), &mut config, 2);
        assert_eq!(
            super::super::workflow_list::handle_event(
                Event::Key(KeyCode::Enter.into()),
                &mut config,
                2,
            ),
            Some(1)
        );

        let reopened =
            ModelConfigStore::open(fixture.root.join("config"), fixture.root.join("keys")).unwrap();
        assert!(matches!(
            Configuration::new(&reopened).page,
            SetupPage::Profiles
        ));
        assert_eq!(reopened.active_config().unwrap().name(), "local");
    }

    #[test]
    fn invalid_form_stays_in_setup_without_creating_a_profile() {
        let mut fixture = TestStore::new();
        let mut config = form();
        config.fields[3] = Input::default();
        super::super::connection_configuration::handle_event(
            Event::Key(KeyCode::F(2).into()),
            &mut config,
            &mut fixture.store,
        );
        assert!(config.error.is_some());
        assert!(matches!(config.page, SetupPage::Form));
        assert!(fixture.store.configs().is_empty());
    }

    #[test]
    fn form_masks_key_and_fits_small_and_large_terminals() {
        let mut config = form();
        config.focused = 3;
        for (width, height) in [(100, 32), (48, 16), (20, 5)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| super::super::connection_configuration::render(frame, &config))
                .unwrap();
            let screen = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(!screen.contains("secret"));
            if width >= 48 {
                assert!(screen.contains("••••••"));
            }
        }
    }
}
