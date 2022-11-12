use actions_toolkit_bindings::{core, io};

pub struct Rustup {}

impl Rustup {
    pub async fn get_or_install() -> Rustup {
        todo!()
    }

    pub async fn get() -> Option<Rustup> {
        let name = "rustup".into();
        let path = io::which(&name, Some(true)).await;
        core::info(format!("{:?}", path));

        {
            let name = "rustup-lljsdfl".into();
            let path = io::which(&name, Some(true)).await;
            core::info(format!("{:?}", path));
        }
        Some(Rustup {})
    }
}
