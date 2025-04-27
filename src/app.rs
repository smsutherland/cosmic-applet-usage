// SPDX-License-Identifier: MPL-2.0

use std::{sync::LazyLock, time::Duration};

use crate::{config::Config, fl};
use cosmic::{
    cosmic_config::{self, CosmicConfigEntry},
    iced::{
        alignment::{Horizontal, Vertical},
        stream, Subscription,
    },
    prelude::*,
    widget::{self, autosize, Id},
};
use futures_util::SinkExt;
use tokio::time::interval;

static AUTOSIZE_MAIN_ID: LazyLock<Id> = LazyLock::new(|| Id::new("autosize-main"));

/// The application model stores app-specific state used to describe its interface and
/// drive its logic.
pub struct UsageApp {
    /// Application state which is managed by the COSMIC runtime.
    core: cosmic::Core,
    // Configuration data that persists between application runs.
    config: Config,
    usage_info: UsageInfo,
}

/// Messages emitted by the application and its widgets.
#[derive(Debug, Clone)]
pub enum Message {
    UpdateConfig(Config),
    Cpu(f32),
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
                    Err((_errors, config)) => {
                        // for why in errors {
                        //     tracing::error!(%why, "error loading app config");
                        // }

                        config
                    }
                })
                .unwrap_or_default(),
            usage_info: UsageInfo { cpu_usage: 0. },
        };

        (app, Task::none())
    }

    /// Describes the interface based on the current state of the application model.
    ///
    /// Application events will be processed through the view. Any messages emitted by
    /// events received by widgets will be passed to the update method.
    fn view(&self) -> Element<Self::Message> {
        autosize::autosize(
            self.core
                .applet
                .text(fl!("cpu", cpu = ((self.usage_info.cpu_usage) as u8)))
                .apply(widget::container)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center),
            AUTOSIZE_MAIN_ID.clone(),
        )
        .into()
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
                    let usage =
                        cpus.iter().map(|cpu| cpu.cpu_usage()).sum::<f32>() / cpus.len() as f32;
                    output.send(Message::Cpu(usage)).await.unwrap();
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
            }
            Message::Cpu(usage) => {
                self.usage_info.cpu_usage = usage;
            }
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}

struct UsageInfo {
    cpu_usage: f32,
}
