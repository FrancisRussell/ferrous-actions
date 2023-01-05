use super::Hook;
use crate::actions::exec::Command;
use crate::core::AnnotationLevel;
use crate::warning;
use async_trait::async_trait;
use cargo_metadata::diagnostic::{DiagnosticLevel, DiagnosticSpan};
use std::borrow::Cow;

#[derive(Default)]
pub struct Annotation {
    subcommand: String,
}

impl Annotation {
    pub fn new(subcommand: &str) -> Annotation {
        Annotation {
            subcommand: subcommand.to_string(),
        }
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
                annotation.title(&format!("cargo-{}: {}", cargo_subcommand, diagnostic.message));
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
        #[allow(clippy::match_same_arms)]
        match level {
            DiagnosticLevel::Ice | DiagnosticLevel::Error => AnnotationLevel::Error,
            DiagnosticLevel::Warning => AnnotationLevel::Warning,
            DiagnosticLevel::FailureNote | DiagnosticLevel::Note | DiagnosticLevel::Help => AnnotationLevel::Notice,
            _ => AnnotationLevel::Warning,
        }
    }

    fn get_primary_span(spans: &[DiagnosticSpan]) -> Option<&DiagnosticSpan> {
        spans.iter().find(|s| s.is_primary)
    }
}

#[async_trait(?Send)]
impl Hook for Annotation {
    fn additional_cargo_options(&self) -> Vec<Cow<str>> {
        vec!["--message-format=json".into()]
    }

    fn modify_command(&self, command: &mut Command) {
        use crate::actions::exec::Stdio;

        let subcommand = self.subcommand.clone();
        command
            .outline(move |line| Self::process_json_record(&subcommand, line))
            .stdout(Stdio::null());
    }
}
