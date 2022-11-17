use crate::actions::exec::Command;
use crate::actions::io;
use crate::node::path::Path;
use crate::Error;

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

    pub async fn run<'a, I>(&'a mut self, subcommand: &'a str, args: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let subcommand = subcommand.to_string();
        let args: Vec<String> = args.into_iter().map(Into::into).collect();
        let mut final_args = Vec::new();
        final_args.push(subcommand);
        final_args.extend(args);

        Command::from(&self.path)
            .args(final_args)
            .exec()
            .await
            .map_err(Error::Js)?;
        Ok(())
    }
}
