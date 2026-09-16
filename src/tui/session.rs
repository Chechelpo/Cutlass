use super::input::Input;
use crate::agent::presets::registry::AgentPresetRegistry;
use crate::agent::sandbox::filesystem::{BindMount, SandboxedFilesystem};
use crate::config::ModelConfig;
use crate::orchestrator::workflow::session::{WorkflowContext, WorkflowDefinition};
use crate::ui_interface::chat::RenderMessageSection;
use crate::utils::default_ro_binds::ro_binds;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::{io, thread};

enum Update {
    Ready,
    Section(RenderMessageSection),
    Finished(Result<(), String>),
    Failed(String),
}

pub(super) struct Session {
    pub workflow: String,
    pub model: String,
    pub sections: Vec<RenderMessageSection>,
    pub composer: Input,
    pub busy: bool,
    pub starting: bool,
    pub disconnected: bool,
    pub error: Option<String>,
    /// Number of wrapped lines above the bottom; zero follows incoming output.
    pub scroll: usize,
    pub tick: usize,
    commands: Sender<String>,
    updates: Receiver<Update>,
}

impl Session {
    pub fn start(
        definition: WorkflowDefinition,
        model: ModelConfig,
        workspace: PathBuf,
    ) -> io::Result<Self> {
        let (commands, requests) = mpsc::channel::<String>();
        let (output, updates) = mpsc::channel();
        let workflow_name = definition.name.clone();
        let model_name = model.id().to_owned();
        thread::Builder::new()
            .name("cutlass-workflow".into())
            .spawn(move || {
                let agents = AgentPresetRegistry::new();
                let context = WorkflowContext {
                    workspace: SandboxedFilesystem::new(
                        ro_binds(),
                        vec![BindMount {
                            host: workspace.clone(),
                            guest: workspace,
                        }],
                    ),
                    model: &model,
                    agents: &agents,
                };
                let mut workflow = match (definition.create)(context) {
                    Ok(workflow) => workflow,
                    Err(error) => {
                        let _ = output.send(Update::Failed(error.message));
                        return;
                    }
                };
                if output.send(Update::Ready).is_err() {
                    return;
                }
                while let Ok(prompt) = requests.recv() {
                    let result = workflow.submit(prompt, &mut |section| {
                        let _ = output.send(Update::Section(section));
                    });
                    if output
                        .send(Update::Finished(result.map_err(|e| e.message)))
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            workflow: workflow_name,
            model: model_name,
            sections: Vec::new(),
            composer: Input::default(),
            busy: true,
            starting: true,
            disconnected: false,
            error: None,
            scroll: 0,
            tick: 0,
            commands,
            updates,
        })
    }

    pub fn poll(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        if self.disconnected {
            return;
        }
        loop {
            match self.updates.try_recv() {
                Ok(Update::Ready) => {
                    self.starting = false;
                    self.busy = false;
                }
                Ok(Update::Section(section)) => self.sections.push(section),
                Ok(Update::Finished(result)) => {
                    self.busy = false;
                    self.error = result.err();
                }
                Ok(Update::Failed(error)) => {
                    self.error = Some(error);
                    self.disconnected = true;
                    self.busy = false;
                    self.starting = false;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.error =
                        Some("The workflow worker stopped. Press Esc to return to setup.".into());
                    self.disconnected = true;
                    self.busy = false;
                    self.starting = false;
                    break;
                }
            }
        }
    }

    pub fn submit(&mut self) {
        if self.busy || self.disconnected || self.composer.text.trim().is_empty() {
            return;
        }
        if self.commands.send(self.composer.text.clone()).is_ok() {
            self.composer.take();
            self.busy = true;
            self.error = None;
            self.scroll = 0;
        } else {
            self.error = Some("The workflow is no longer running.".into());
            self.disconnected = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::workflow::{session::WorkflowSession, workflow::WorkflowError};
    use crate::ui_interface::chat::{RenderText, RenderToolCall, RenderToolGroup};
    use std::time::{Duration, Instant};

    struct AlternateWorkflow;
    impl WorkflowSession for AlternateWorkflow {
        fn submit(
            &mut self,
            prompt: String,
            emit: &mut dyn FnMut(RenderMessageSection),
        ) -> Result<(), WorkflowError> {
            if prompt == "fail" {
                return Err(WorkflowError {
                    message: "Provider rejected request".into(),
                    retryable: false,
                });
            }
            emit(RenderMessageSection::Message {
                speaker: "Alternate".into(),
                content: RenderText::markdown(prompt),
            });
            emit(RenderMessageSection::ToolGroup(RenderToolGroup::new(
                RenderText::plain("Files"),
                vec![RenderToolCall::new(RenderText::plain("Read"))],
            )));
            Ok(())
        }
    }

    fn wait_for_idle(session: &mut Session) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while session.busy && Instant::now() < deadline {
            session.poll();
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!session.busy);
    }

    fn model() -> ModelConfig {
        serde_json::from_value(serde_json::json!({
            "name": "test", "host_url": "http://localhost/v1", "id": "test", "encrypted_keys": [],
            "max_input_tokens": 100, "max_output_tokens": 50
        }))
        .unwrap()
    }

    fn alternate() -> Session {
        let definition = WorkflowDefinition {
            name: "Alternate".into(),
            description: "test".into(),
            create: |_| Ok(Box::new(AlternateWorkflow)),
        };
        Session::start(definition, model(), PathBuf::from("/tmp")).unwrap()
    }

    #[test]
    fn starts_selected_factory_and_delivers_live_sections() {
        let mut session = alternate();
        wait_for_idle(&mut session);
        assert!(session.error.is_none());
        session.composer.insert("hello", true);
        session.submit();
        assert!(session.busy);
        wait_for_idle(&mut session);
        assert!(session.error.is_none());
        assert_eq!(session.sections.len(), 2);
        assert!(
            matches!(&session.sections[0], RenderMessageSection::Message { speaker, .. } if speaker == "Alternate")
        );
        assert!(matches!(
            &session.sections[1],
            RenderMessageSection::ToolGroup(_)
        ));
    }

    #[test]
    fn worker_errors_allow_recovery_without_losing_history() {
        let mut session = alternate();
        wait_for_idle(&mut session);
        session.composer.insert("hello", true);
        session.submit();
        wait_for_idle(&mut session);
        session.composer.insert("fail", true);
        session.submit();
        wait_for_idle(&mut session);
        assert_eq!(session.error.as_deref(), Some("Provider rejected request"));
        assert_eq!(session.sections.len(), 2);
        assert!(!session.disconnected);
        session.composer.insert("recover", true);
        session.submit();
        wait_for_idle(&mut session);
        assert!(session.error.is_none());
        assert_eq!(session.sections.len(), 4);
    }

    #[test]
    fn factory_failure_is_visible_and_session_layout_handles_resize() {
        let definition = WorkflowDefinition {
            name: "Unavailable".into(),
            description: "test".into(),
            create: |_| {
                Err(WorkflowError {
                    message: "Missing preset".into(),
                    retryable: false,
                })
            },
        };
        let mut session = Session::start(definition, model(), PathBuf::from("/tmp")).unwrap();
        wait_for_idle(&mut session);
        assert_eq!(session.error.as_deref(), Some("Missing preset"));
        assert!(session.disconnected);
        session
            .composer
            .insert("long draft\nwith\nseveral\nlines\nand 界🙂", true);
        for (width, height) in [(100, 32), (48, 16), (20, 5)] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| super::super::view::session(frame, &mut session, "/workspace"))
                .unwrap();
            let screen = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            if width >= 48 {
                assert!(screen.contains("Missing preset"));
            }
        }
    }
}
