// SPDX-License-Identifier: GPL-3.0-only

mod wayland;

use std::sync::LazyLock;

use cosmic::{
    Element, app,
    applet::cosmic_panel_config::PanelAnchor,
    desktop::{
        DesktopEntryCache, DesktopLookupContext, DesktopResolveOptions, IconSourceExt, fde,
        resolve_desktop_entry,
    },
    iced::{
        self, Alignment, Length, Subscription,
        widget::{row, space},
    },
    theme,
    widget::{self, autosize, container},
};

use wayland::{WaylandUpdate, WorkspaceWindow, workspace_windows_subscription};

const APP_ID: &str = "io.github.tkilian.CosmicAppletAppTitle";
const HORIZONTAL_MAX_CHARS: usize = 24;
const VERTICAL_MAX_CHARS: usize = 14;
const EMPTY_TITLE: &str = "Desktop";

static AUTOSIZE_MAIN_ID: LazyLock<widget::Id> = LazyLock::new(|| widget::Id::new("autosize-main"));

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<Applet>(())
}

#[derive(Clone)]
struct DisplayWindow {
    title: String,
    icon: Option<widget::icon::Handle>,
    is_active: bool,
}

pub struct Applet {
    core: cosmic::app::Core,
    desktop_cache: DesktopEntryCache,
    windows: Vec<DisplayWindow>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Wayland(WaylandUpdate),
}

impl Applet {
    fn max_chars(&self) -> usize {
        match self.core.applet.anchor {
            PanelAnchor::Left | PanelAnchor::Right => VERTICAL_MAX_CHARS,
            PanelAnchor::Top | PanelAnchor::Bottom => HORIZONTAL_MAX_CHARS,
        }
    }

    fn resolve_icon(&mut self, window: &WorkspaceWindow) -> Option<widget::icon::Handle> {
        let app_id = window.app_id.as_deref().or(window.identifier.as_deref())?;

        let mut lookup = DesktopLookupContext::new(app_id).with_title(window.title.as_str());
        if let Some(identifier) = window.identifier.as_deref() {
            lookup = lookup.with_identifier(identifier);
        }

        let entry = resolve_desktop_entry(
            &mut self.desktop_cache,
            &lookup,
            &DesktopResolveOptions::default(),
        );
        let icon = fde::IconSource::from_unknown(entry.icon().unwrap_or(&entry.appid));
        Some(icon.as_cosmic_icon())
    }

    fn display_windows(
        &self,
    ) -> impl Iterator<Item = (&str, Option<widget::icon::Handle>, bool)> + '_ {
        self.windows
            .iter()
            .map(|window| (window.title.as_str(), window.icon.clone(), window.is_active))
    }

    fn window_chip(
        &self,
        title: &str,
        icon: Option<widget::icon::Handle>,
        is_active: bool,
        icon_size: f32,
    ) -> Element<'_, Message> {
        let text = truncate_title(title, self.max_chars());
        let mut content = row![].align_y(Alignment::Center).spacing(4);

        if let Some(icon) = icon {
            content = content.push(
                widget::icon(icon)
                    .width(Length::Fixed(icon_size))
                    .height(Length::Fixed(icon_size)),
            );
        }

        content = content.push(self.core.applet.text(text));

        container(content)
            .padding([2, 8])
            .class(theme::Container::custom(move |theme| {
                let cosmic = theme.cosmic();
                let (background, foreground) = if is_active {
                    (
                        cosmic.accent_button.base.into(),
                        cosmic.accent_button.on.into(),
                    )
                } else {
                    (
                        cosmic.background.component.base.into(),
                        cosmic.background.component.on.into(),
                    )
                };

                container::Style {
                    icon_color: Some(foreground),
                    text_color: Some(foreground),
                    background: Some(iced::Background::Color(background)),
                    border: iced::Border {
                        radius: cosmic.corner_radii.radius_s.into(),
                        ..Default::default()
                    },
                    shadow: Default::default(),
                    snap: true,
                }
            }))
            .into()
    }
}

impl cosmic::Application for Applet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();

    const APP_ID: &'static str = APP_ID;

    fn init(core: cosmic::app::Core, _flags: Self::Flags) -> (Self, app::Task<Self::Message>) {
        (
            Self {
                core,
                desktop_cache: DesktopEntryCache::new(fde::get_languages_from_env()),
                windows: Vec::new(),
            },
            app::Task::none(),
        )
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        match message {
            Message::Wayland(update) => match update {
                WaylandUpdate::WorkspaceWindows(windows) => {
                    self.windows = windows
                        .into_iter()
                        .map(|window| DisplayWindow {
                            title: window.title.clone(),
                            icon: self.resolve_icon(&window),
                            is_active: window.is_active,
                        })
                        .collect();
                }
                WaylandUpdate::Finished => {
                    tracing::error!("Wayland subscription ended");
                }
            },
        }

        app::Task::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        workspace_windows_subscription().map(Message::Wayland)
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let height = (self.core.applet.suggested_size(true).1
            + 2 * self.core.applet.suggested_padding(true).1) as f32;
        let icon_size = self.core.applet.suggested_size(true).0 as f32;
        let mut content = row![].align_y(Alignment::Center).spacing(6);

        if self.windows.is_empty() {
            content = content.push(self.window_chip(EMPTY_TITLE, None, false, icon_size));
        } else {
            for (title, icon, is_active) in self.display_windows() {
                content = content.push(self.window_chip(title, icon, is_active, icon_size));
            }
        }

        content = content.push(space::vertical().height(Length::Fixed(height)));

        let content = container(content).padding([0, self.core.applet.suggested_padding(true).0]);

        autosize::autosize(content, AUTOSIZE_MAIN_ID.clone()).into()
    }
}

fn truncate_title(title: &str, max_chars: usize) -> String {
    let char_count = title.chars().count();
    if char_count <= max_chars {
        return title.to_owned();
    }

    let keep = max_chars.saturating_sub(3);
    let mut truncated = title.chars().take(keep).collect::<String>();
    truncated.push_str("...");
    truncated
}
