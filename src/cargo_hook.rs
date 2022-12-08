use crate::actions::exec::Command;
use async_trait::async_trait;
use std::borrow::Cow;

#[async_trait(?Send)]
pub trait CargoHook {
    fn additional_cargo_options(&self) -> Vec<Cow<str>> {
        Vec::new()
    }

    fn modify_command(&self, command: &mut Command) {
        let _ = command;
    }

    async fn succeeded(&mut self) {}
    async fn failed(&mut self) {}
}

#[derive(Default)]
pub struct CompositeCargoHook<'a> {
    hooks: Vec<Box<dyn CargoHook + Sync + 'a>>,
}

impl<'a> CompositeCargoHook<'a> {
    pub fn push<H: CargoHook + Sync + 'a>(&mut self, hook: H) {
        self.hooks.push(Box::new(hook));
    }
}

#[async_trait(?Send)]
impl<'a> CargoHook for CompositeCargoHook<'a> {
    fn additional_cargo_options(&self) -> Vec<Cow<str>> {
        let mut result = Vec::new();
        for hook in &self.hooks {
            result.extend(hook.additional_cargo_options());
        }
        result
    }

    fn modify_command(&self, command: &mut Command) {
        for hook in &self.hooks {
            hook.modify_command(command);
        }
    }

    async fn succeeded(&mut self) {
        for hook in self.hooks.iter_mut().rev() {
            hook.succeeded().await;
        }
    }

    async fn failed(&mut self) {
        for hook in self.hooks.iter_mut().rev() {
            hook.failed().await;
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct NullHook {}

impl CargoHook for NullHook {}
