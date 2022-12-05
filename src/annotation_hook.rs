use crate::actions::exec::Command;
use crate::cargo_hook::CargoHook;
use crate::core;
use crate::core::AnnotationLevel;
use crate::warning;
use crate::Error;
use async_trait::async_trait;
use cargo_metadata::diagnostic::DiagnosticLevel;
use cargo_metadata::diagnostic::DiagnosticSpan;
use std::borrow::Cow;

#[derive(Default)]
pub struct AnnotationHook {
    enabled: bool,
    subcommand: String,
}

impl AnnotationHook {
    pub async fn new(subcommand: &str) -> Result<AnnotationHook, Error> {
        let enabled = if let Some(enabled) = core::get_input("annotations")? {
            enabled
                .parse::<bool>()
                .map_err(|_| Error::OptionParseError("annotations".into(), enabled.clone()))?
        } else {
            true
        };
        let result = AnnotationHook {
            enabled,
            subcommand: subcommand.to_string(),
        };
        Ok(result)
    }

    fn process_json_record(cargo_subcommand: &str, line: &str) {
        use crate::core::Annotation;
        use crate::node::path::Path;
        use cargo_metadata::Message;

        let metadata: Message = match serde_json::from_str(line) {
            Ok(metadata) => metadata,
            Err(e) => {
                warning!("Unable to cargo output line as JSON metadata record: {}", e);
                return;
            }
        };
        if let Message::CompilerMessage(compiler_message) = metadata {
            let diagnostic = &compiler_message.message;
            let level = Self::annotation_level(diagnostic.level);
            let mut annotation = if let Some(rendered) = &diagnostic.rendered {
                let mut annotation = Annotation::from(rendered.as_str());
                annotation.title(&format!(
                    "cargo-{}: {}",
                    cargo_subcommand, diagnostic.message
                ));
                annotation
            } else {
                let mut annotation = Annotation::from(diagnostic.message.as_str());
                annotation.title(&format!("cargo-{}", cargo_subcommand));
                annotation
            };
            if let Some(span) = Self::get_primary_span(&diagnostic.spans) {
                let file_name = Path::from(span.file_name.as_str());
                annotation
                    .file(&file_name)
                    .start_line(span.line_start)
                    .end_line(span.line_end)
                    .start_column(span.column_start)
                    .end_column(span.column_end);
            }
            annotation.output(level);
        }
    }

    fn annotation_level(level: DiagnosticLevel) -> AnnotationLevel {
        match level {
            DiagnosticLevel::Ice => AnnotationLevel::Error,
            DiagnosticLevel::Error => AnnotationLevel::Error,
            DiagnosticLevel::Warning => AnnotationLevel::Warning,
            DiagnosticLevel::FailureNote => AnnotationLevel::Notice,
            DiagnosticLevel::Note => AnnotationLevel::Notice,
            DiagnosticLevel::Help => AnnotationLevel::Notice,
            _ => AnnotationLevel::Warning,
        }
    }

    fn get_primary_span(spans: &[DiagnosticSpan]) -> Option<&DiagnosticSpan> {
        spans.iter().find(|s| s.is_primary)
    }
}

#[async_trait]
impl CargoHook for AnnotationHook {
    fn additional_cargo_options(&self) -> Vec<Cow<str>> {
        if self.enabled {
            vec!["--message-format=json".into()]
        } else {
            Vec::new()
        }
    }

    fn modify_command(&self, command: &mut Command) {
        use crate::actions::exec::Stdio;

        let subcommand = self.subcommand.clone();
        command
            .outline(move |line| Self::process_json_record(&subcommand, line))
            .stdout(Stdio::null());
    }

    async fn succeeded(&mut self) {}

    async fn failed(&mut self) {}
}
