use log::{warn, trace};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fmt,
    sync::Arc,
};

use crate::{
    metadata::PluginMetadata,
    runtime::{Config, ConfigValue, MylifeComponent, MylifePluginRuntime, Value},
    MylifePlugin,
};

pub struct PluginRuntimeImpl<PluginType: MylifePlugin + 'static> {
    metadata: PluginMetadata,
    access: Arc<PluginRuntimeAccess<PluginType>>,
}

impl<PluginType: MylifePlugin + 'static> PluginRuntimeImpl<PluginType> {
    pub fn new(
        metadata: PluginMetadata,
        access: Arc<PluginRuntimeAccess<PluginType>>,
    ) -> Box<Self> {
        Box::new(PluginRuntimeImpl { metadata, access })
    }
}

impl<PluginType: MylifePlugin> MylifePluginRuntime for PluginRuntimeImpl<PluginType> {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn create(&self, id: &str) -> Box<dyn MylifeComponent> {
        ComponentImpl::<PluginType>::new(&self.access, id)
    }
}

pub struct StateRuntime<PluginType> {
    pub(crate) register: StateRuntimeRegister<PluginType>,
    pub(crate) getter: StateRuntimeGetter<PluginType>,
}

pub type ConfigRuntimeSetter<PluginType> =
    fn(target: &mut PluginType, config: ConfigValue) -> Result<(), Box<dyn std::error::Error>>;
pub type StateRuntimeRegister<PluginType> =
    fn(target: &mut PluginType, listener: Box<dyn Fn(Value)>) -> ();
pub type StateRuntimeGetter<PluginType> = fn(target: &PluginType) -> Value;
pub type ActionRuntimeExecutor<PluginType> =
    fn(target: &mut PluginType, action: Value) -> Result<(), Box<dyn std::error::Error>>;

pub struct PluginRuntimeAccess<PluginType: MylifePlugin> {
    configs: HashMap<String, ConfigRuntimeSetter<PluginType>>,
    states: HashMap<String, StateRuntime<PluginType>>,
    actions: HashMap<String, ActionRuntimeExecutor<PluginType>>,
}

impl<PluginType: MylifePlugin> PluginRuntimeAccess<PluginType> {
    pub fn new(
        configs: HashMap<String, ConfigRuntimeSetter<PluginType>>,
        states: HashMap<String, StateRuntime<PluginType>>,
        actions: HashMap<String, ActionRuntimeExecutor<PluginType>>,
    ) -> Arc<Self> {
        Arc::new(PluginRuntimeAccess {
            configs,
            states,
            actions,
        })
    }
}

/// Component runtime failure manager
struct FailureHandler {
    failure: Cell<Option<Box<dyn std::error::Error>>>,
    fail_handler: RefCell<Box<dyn Fn(/*error:*/ &Box<dyn std::error::Error>)>>,
}

impl FailureHandler {
    fn new() -> Arc<Self> {
        Arc::new(FailureHandler {
            failure: Cell::new(None),
            fail_handler: RefCell::new(Box::new(|_error| {
                panic!("Cannot report error without registered fail handler");
            })),
        })
    }

    fn fail(&self, error: Box<dyn std::error::Error>) {
        if self.failure().is_some() {
            // do not overwrite failure because we deref it aggressively
            warn!(target: "mylife:home:core:plugin-runtime:macros-backend:runtime", "Cannot overwrite previous failure, ignoring {error}");
        } else {
            warn!(target: "mylife:home:core:plugin-runtime:macros-backend:runtime", "Fail {error}");
            self.failure.set(Some(error));
        }

        self.fail_handler.borrow()(self.failure().as_ref().unwrap());
    }

    /// Set the target handler that will be called on failure
    fn set_handler(&self, handler: Box<dyn Fn(/*error:*/ &Box<dyn std::error::Error>)>) {
        *self.fail_handler.borrow_mut() = handler;
    }

    fn failure<'a>(&'a self) -> Option<&'a Box<dyn std::error::Error>> {
        // failure may only be set once, not overwritten afterward.
        // So if there is a failure, it is safe to get its ref as long as the FailureHandler lives
        let failure_ref = unsafe { &*self.failure.as_ptr() };
        failure_ref.as_ref()
    }

    /// Build a closure that will call self.fail
    fn make_handler(self: &Arc<Self>) -> Box<dyn Fn(Box<dyn std::error::Error>)> {
        let handler = self.clone();
        Box::new(move |error: Box<dyn std::error::Error>| {
            handler.fail(error);
        })
    }
}

struct ComponentImpl<PluginType: MylifePlugin> {
    access: Arc<PluginRuntimeAccess<PluginType>>,
    component: PluginType,
    id: String,
    failure_handler: Arc<FailureHandler>,
    state_handler: Arc<RefCell<Option<Box<dyn Fn(/*name:*/ &str, /*value:*/ Value)>>>>,
}

impl<PluginType: MylifePlugin> ComponentImpl<PluginType> {
    pub fn new(access: &Arc<PluginRuntimeAccess<PluginType>>, id: &str) -> Box<Self> {
        let failure_handler = FailureHandler::new();

        let mut component = Box::new(ComponentImpl {
            access: access.clone(),
            component: PluginType::new(id, failure_handler.make_handler()),
            id: String::from(id),
            failure_handler,
            state_handler: Arc::new(RefCell::new(None)),
        });

        component.register_state_handlers();

        component
    }

    fn register_state_handlers(&mut self) {
        for (name, state) in self.access.states.iter() {
            let id = self.id.clone();
            let name = name.clone();
            let state_handler = self.state_handler.clone();
            (state.register)(
                &mut self.component,
                Box::new(move |value: Value| {
                    trace!(target: "mylife:home:core:plugin-runtime:macros-backend:runtime", "[{id}] state '{name}' changed to {value:?}");

                    if let Some(handler) = state_handler.borrow().as_ref() {
                        handler(&name, value);
                    }
                }),
            );
        }
    }

    // TODO: better error type
    fn configure_with_res(&mut self, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
        trace!(target: "mylife:home:core:plugin-runtime:macros-backend:runtime", "[{}] configure with {config:?}", self.id);

        for (name, setter) in self.access.configs.iter() {
            let value = config
                .get(name)
                .ok_or_else(|| {
                    Box::new(ConfigNotSetError {
                        name: String::from(name),
                    })
                })?
                .clone();

            setter(&mut self.component, value)?;
        }

        Ok(())
    }

    // TODO: better error type
    fn execute_action_with_res(
        &mut self,
        name: &str,
        action: Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let handler = self.access.actions.get(name).ok_or_else(|| {
            Box::new(NoSuchActionError {
                name: String::from(name),
            })
        })?;

        trace!(target: "mylife:home:core:plugin-runtime:macros-backend:runtime", "[{}] execute action '{name}' with {action:?}", self.id);
        handler(&mut self.component, action)
    }

    fn res_to_fail<T>(&mut self, result: Result<T, Box<dyn std::error::Error>>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.failure_handler.fail(error);
                None
            }
        }
    }
}

impl<PluginType: MylifePlugin> MylifeComponent for ComponentImpl<PluginType> {
    fn id(&self) -> &str {
        &self.id
    }

    fn set_on_fail(&mut self, handler: Box<dyn Fn(/*error:*/ &Box<dyn std::error::Error>)>) {
        self.failure_handler.set_handler(handler);
    }

    fn failure(&self) -> Option<&Box<dyn std::error::Error>> {
        self.failure_handler.failure()
    }

    fn set_on_state(&mut self, handler: Box<dyn Fn(/*name:*/ &str, /*value:*/ Value)>) {
        *self.state_handler.borrow_mut() = Some(handler);
    }

    fn get_state(&self, name: &str) -> Result<Value, Box<dyn std::error::Error>> {
        let state = self.access.states.get(name).ok_or_else(|| {
            Box::new(NoSuchStateError {
                name: String::from(name),
            })
        })?;

        Ok((state.getter)(&self.component))
    }

    fn configure(&mut self, config: &Config) {
        let result = self.configure_with_res(config);
        self.res_to_fail(result);
    }

    fn init(&mut self) {
        let result = self.component.init();
        self.res_to_fail(result);
    }

    fn execute_action(&mut self, name: &str, action: Value) {
        let result = self.execute_action_with_res(name, action);
        self.res_to_fail(result);
    }
}

#[derive(Debug, Clone)]
pub struct ConfigNotSetError {
    name: String,
}

impl std::error::Error for ConfigNotSetError {}

impl fmt::Display for ConfigNotSetError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Config key not set: '{}'", self.name)
    }
}

#[derive(Debug, Clone)]
pub struct NoSuchActionError {
    name: String,
}

impl std::error::Error for NoSuchActionError {}

impl fmt::Display for NoSuchActionError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "No such action: '{}'", self.name)
    }
}

#[derive(Debug, Clone)]
pub struct NoSuchStateError {
    name: String,
}

impl std::error::Error for NoSuchStateError {}

impl fmt::Display for NoSuchStateError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "No such state: '{}'", self.name)
    }
}
