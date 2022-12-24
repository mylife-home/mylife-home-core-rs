use std::fmt;
use log::{debug};

use plugin_macros::{mylife_actions, MylifePlugin};
use plugin_runtime::{MylifePlugin, MylifePluginHooks, State};

#[derive(MylifePlugin, Default)]
#[mylife_plugin(description = "step relay", usage = "logic")] // name=
pub struct ValueBinary {
    #[mylife_config(description = "initial value (useless only config example")] // type=, name=
    config: bool,

    #[mylife_state(description = "actual value")] // type=, name=
    state: State<bool>,
}

// impl Drop si besoin de terminate
impl MylifePluginHooks for ValueBinary {
    fn init(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.state.set(self.config);

        debug!(target: "mylife:home:core:plugins:logic-base:value-binary", component = self.name(); "initial state = {}", self.state.get());

        Ok(())
    }
}

#[mylife_actions]
impl ValueBinary {
    // can return Result<(), Box<dyn std::error::Error>> or nothing
    #[mylife_action(description = "set value to on")] // type=, name=
    fn on(&mut self, arg: bool) -> Result<(), Box<dyn std::error::Error>> {
        if arg {
            self.state.set(true);
        }

        Ok(())
    }

    #[mylife_action(description = "set value to off")]
    fn off(&mut self, arg: bool) {
        if arg {
            self.state.set(false);
        }
    }

    #[mylife_action(description = "toggle value")]
    fn toggle(&mut self, arg: bool) {
        self.fail(Box::new(TestError()));
        return;

        if arg {
            self.state.set(!self.state.get());
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestError();

impl std::error::Error for TestError {}

impl fmt::Display for TestError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Boom!",)
    }
}
