use crate::actions::core::{Annotation, AnnotationLevel};
use crate::actions::exec::{Command, Stdio};
use crate::actions::{core, io};
use crate::info;
use crate::node::path::Path;
use crate::Error;
use cargo_metadata::diagnostic::{DiagnosticLevel, DiagnosticSpan};

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

    fn supports_json_message_format(subcommand: &str) -> bool {
        ["build", "check", "clippy"]
            .iter()
            .any(|s| s == &subcommand)
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

    pub async fn run<'a, I>(
        &'a mut self,
        toolchain: Option<&str>,
        subcommand: &'a str,
        args: I,
    ) -> Result<(), Error>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let subcommand = subcommand.to_string();
        let args: Vec<String> = args.into_iter().map(Into::into).collect();
        let mut final_args = Vec::new();
        if let Some(toolchain) = toolchain {
            final_args.push(format!("+{}", toolchain));
        }

        let annotations_enabled = Self::supports_json_message_format(subcommand.as_str());
        let annotations_enabled = annotations_enabled
            && if let Some(enabled) = core::get_input("annotations")? {
                enabled
                    .parse::<bool>()
                    .map_err(|_| Error::OptionParseError("annotations".into(), enabled.clone()))?
            } else {
                true
            };
        final_args.push(subcommand.clone());
        if annotations_enabled {
            final_args.push("--message-format=json".into());
        }
        final_args.extend(args);

        let mut command = Command::from(&self.path);
        command.args(final_args);
        if annotations_enabled {
            let subcommand = subcommand.to_string();
            command
                .outline(move |line| Self::process_json_record(&subcommand, line))
                .stdout(Stdio::null());
        }
        command.exec().await.map_err(Error::Js)?;
        Ok(())
    }
}
