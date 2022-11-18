use crate::actions::core::{Annotation, AnnotationLevel};
use crate::actions::exec::{Command, Stdio};
use crate::actions::io;
use crate::info;
use crate::node::path::Path;
use crate::Error;
use cargo_metadata::diagnostic::DiagnosticSpan;

#[derive(Debug)]
pub struct Cargo {
    path: Path,
}

impl Cargo {
    pub async fn from_environment() -> Result<Cargo, Error> {
        io::which("cargo", true)
            .await
            .map(|path| Cargo { path })
            .map_err(Error::Js)
    }

    fn annotation_level(level: cargo_metadata::diagnostic::DiagnosticLevel) -> AnnotationLevel {
        use cargo_metadata::diagnostic::DiagnosticLevel;

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

    fn process_json_record(cargo_subcommand: &str, line: &str) {
        use cargo_metadata::Message;

        let metadata: Message = match serde_json::from_str(line) {
            Ok(metadata) => metadata,
            Err(e) => {
                info!("Unable to cargo output line as JSON metadata record: {}", e);
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
                annotation.file(&file_name);
                annotation.start_line(span.line_start);
                annotation.end_line(span.line_end);
                annotation.start_column(span.column_start);
                annotation.end_column(span.column_end);
            }
            annotation.output(level);
        }
    }

    pub async fn run<'a, I>(&'a mut self, subcommand: &'a str, args: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let subcommand = subcommand.to_string();
        let args: Vec<String> = args.into_iter().map(Into::into).collect();
        let mut final_args = Vec::new();
        final_args.push(subcommand.clone());
        let process_json = if subcommand == "clippy" {
            final_args.push("--message-format=json".into());
            true
        } else {
            false
        };
        final_args.extend(args);

        let mut command = Command::from(&self.path);
        command.args(final_args);
        if process_json {
            let subcommand = subcommand.to_string();
            command.outline(move |line| Self::process_json_record(&subcommand, line));
            command.stdout(Stdio::null());
        }
        command.exec().await.map_err(Error::Js)?;
        Ok(())
    }
}
