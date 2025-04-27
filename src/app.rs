// SPDX-License-Identifier: MPL-2.0

use std::{sync::LazyLock, time::Duration};

use crate::{config::Config, fl};
use cosmic::{
    cosmic_config::{self, CosmicConfigEntry},
    iced::{stream, window, Subscription},
    iced_widget::column,
    iced_winit::commands::popup::{destroy_popup, get_popup},
    prelude::*,
    widget::{autosize, button, checkbox, container, Id, Row},
};
use futures_util::SinkExt;
use tokio::time::interval;

static AUTOSIZE_MAIN_ID: LazyLock<Id> = LazyLock::new(|| Id::new("autosize-main"));

/// The application model stores app-specific state used to describe its interface and
/// drive its logic.
#[derive(Default)]
pub struct UsageApp {
    /// Application state which is managed by the COSMIC runtime.
    core: cosmic::Core,
    // Configuration data that persists between application runs.
    config: Config,
    usage_info: UsageInfo,
    popup: Option<window::Id>,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    UpdateConfig(Config),
    UsageUpdate {
        cpu: Option<f32>,
        mem: Option<f32>,
        swap: Option<f32>,
    },
    TogglePopup,
    ToggleElement(UsageElement),
}

/// Create a COSMIC application from the app model
impl cosmic::Application for UsageApp {
    /// The async executor that will be used to run your application's commands.
    type Executor = cosmic::executor::Default;

    /// Data that your application receives to its init method.
    type Flags = ();

    /// Messages which the application and its widgets will emit.
    type Message = Message;

    /// Unique identifier in RDNN (reverse domain name notation) format.
    const APP_ID: &'static str = "com.github.smsutherland.cosmic-applet-usage";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    /// Initializes the application with any given flags and startup commands.
    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        // Construct the app model with the runtime's core.
        let app = UsageApp {
            core,
            // Optional configuration file for an application.
            config: cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                .map(|context| match Config::get_entry(&context) {
                    Ok(config) => config,
                    Err((_errors, config)) => config,
                })
                .unwrap_or_default(),
            ..Default::default()
        };

        (app, Task::none())
    }

    /// Describes the interface based on the current state of the application model.
    ///
    /// Application events will be processed through the view. Any messages emitted by
    /// events received by widgets will be passed to the update method.
    fn view(&self) -> Element<Self::Message> {
        let cpu = self
            .core
            .applet
            .text(fl!("cpu", cpu = ((self.usage_info.cpu) as u8)));

        let memory = self
            .core
            .applet
            .text(fl!("memory", mem = ((self.usage_info.memory * 100.) as u8)));

        let swap = self
            .core
            .applet
            .text(fl!("swap", swap = ((self.usage_info.swap * 100.) as u8)));

        let mut row = Row::new().spacing(5);
        if self.config.cpu_enabled {
            row = row.push(cpu);
        }
        if self.config.memory_enabled {
            row = row.push(memory);
        }
        if self.config.swap_enabled {
            row = row.push(swap);
        };

        let btn = button::custom(row)
            .on_press(Message::TogglePopup)
            .class(cosmic::theme::Button::AppletIcon);

        autosize::autosize(btn, AUTOSIZE_MAIN_ID.clone()).into()
    }

    fn view_window(&self, _id: window::Id) -> Element<Self::Message> {
        let col = column![
            checkbox("CPU", self.config.cpu_enabled)
                .on_toggle(|_| Message::ToggleElement(UsageElement::Cpu)),
            checkbox("Memory", self.config.memory_enabled)
                .on_toggle(|_| Message::ToggleElement(UsageElement::Memory)),
            checkbox("Swap", self.config.swap_enabled)
                .on_toggle(|_| Message::ToggleElement(UsageElement::Swap)),
        ]
        .apply(container);
        self.core.applet.popup_container(col).into()
    }

    /// Register subscriptions for this application.
    ///
    /// Subscriptions are long-running async tasks running in the background which
    /// emit messages to the application through a channel. They are started at the
    /// beginning of the application, and persist through its lifetime.
    fn subscription(&self) -> Subscription<Self::Message> {
        let sysinfo = Subscription::run_with_id(
            "sysinfo-sub",
            stream::channel(1, async move |mut output| {
                let mut sys = sysinfo::System::new_all();
                let mut interval = interval(Duration::from_secs(1));
                loop {
                    interval.tick().await;

                    sys.refresh_cpu_usage();
                    let cpus = sys.cpus();
                    let cpu_usage =
                        cpus.iter().map(|cpu| cpu.cpu_usage()).sum::<f32>() / cpus.len() as f32;

                    sys.refresh_memory();
                    let memory_usage =
                        1. - sys.available_memory() as f32 / sys.total_memory() as f32;
                    let swap_usage = 1. - sys.free_swap() as f32 / sys.total_swap() as f32;

                    let message = Message::UsageUpdate {
                        cpu: Some(cpu_usage),
                        mem: Some(memory_usage),
                        swap: Some(swap_usage),
                    };

                    output.send(message).await.unwrap();
                }
            }),
        );

        Subscription::batch(vec![
            sysinfo,
            // Watch for application configuration changes.
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| {
                    // for why in update.errors {
                    //     tracing::error!(?why, "app config error");
                    // }

                    Message::UpdateConfig(update.config)
                }),
        ])
    }

    /// Handles messages emitted by the application and its widgets.
    ///
    /// Tasks may be returned for asynchronous execution of code in the background
    /// on the application's async runtime.
    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::UpdateConfig(config) => {
                self.config = config;
                Task::none()
            }
            Message::UsageUpdate { cpu, mem, swap } => {
                if let Some(cpu) = cpu {
                    self.usage_info.cpu = cpu;
                }
                if let Some(mem) = mem {
                    self.usage_info.memory = mem;
                }
                if let Some(swap) = swap {
                    self.usage_info.swap = swap;
                }
                Task::none()
            }
            Message::TogglePopup => {
                if let Some(id) = self.popup.take() {
                    destroy_popup(id)
                } else {
                    let new_id = window::Id::unique();
                    self.popup.replace(new_id);
                    let popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );

                    get_popup(popup_settings)
                }
            }
            Message::ToggleElement(e) => {
                match e {
                    UsageElement::Cpu => self.config.cpu_enabled = !self.config.cpu_enabled,
                    UsageElement::Memory => {
                        self.config.memory_enabled = !self.config.memory_enabled
                    }
                    UsageElement::Swap => self.config.swap_enabled = !self.config.swap_enabled,
                }
                if let Ok(config) = cosmic_config::Config::new(Self::APP_ID, Config::VERSION) {
                    // If writing the config fails, we still want to continue.
                    // If I start using tracing, then I'll want to log something.
                    let _ = self.config.write_entry(&config);
                }
                Task::none()
            }
        }
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct UsageInfo {
    cpu: f32,
    memory: f32,
    swap: f32,
}

#[derive(Debug, Clone, Copy)]
pub enum UsageElement {
    Cpu,
    Memory,
    Swap,
}
