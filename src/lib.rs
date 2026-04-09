// SPDX-License-Identifier: GPL-3.0-only

mod wayland;

use std::sync::LazyLock;

use cosmic::{
    cctk::wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
    Element, app,
    applet::cosmic_panel_config::PanelAnchor,
    desktop::{
        DesktopEntryCache, DesktopLookupContext, DesktopResolveOptions, IconSourceExt, fde,
        resolve_desktop_entry,
    },
    iced::{
        self, Alignment, Length, Subscription, event, mouse,
        widget::{row, space, stack},
    },
    theme,
    widget::{self, autosize, container},
};

use wayland::{
    WaylandUpdate, WorkspaceWindow, close_window, focus_window, workspace_windows_subscription,
};

const APP_ID: &str = "io.github.tkilian.CosmicAppletAppTitle";
const CLOSE_ICON_SIZE: u16 = 14;
const HORIZONTAL_MAX_CHARS: usize = 24;
const VERTICAL_MAX_CHARS: usize = 14;
const EMPTY_TITLE: &str = "Desktop";

static AUTOSIZE_MAIN_ID: LazyLock<widget::Id> = LazyLock::new(|| widget::Id::new("autosize-main"));

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<Applet>(())
}

#[derive(Clone)]
struct DisplayWindow {
    handle: ExtForeignToplevelHandleV1,
    title: String,
    icon: Option<widget::icon::Handle>,
    is_active: bool,
}

pub struct Applet {
    core: cosmic::app::Core,
    desktop_cache: DesktopEntryCache,
    hovered_window: Option<ExtForeignToplevelHandleV1>,
    windows: Vec<DisplayWindow>,
}

#[derive(Debug, Clone)]
pub enum Message {
    ClearHoveredWindow(ExtForeignToplevelHandleV1),
    ClearHoveredWindowGlobal,
    CloseWindow(ExtForeignToplevelHandleV1),
    FocusWindow(ExtForeignToplevelHandleV1),
    HoverWindow(ExtForeignToplevelHandleV1),
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

    fn close_button(
        handle: ExtForeignToplevelHandleV1,
        is_active: bool,
    ) -> Element<'static, Message> {
        widget::button::custom(
            widget::icon::from_name("window-close-symbolic")
                .size(CLOSE_ICON_SIZE)
                .icon(),
        )
        .padding(4)
        .class(close_button_class(is_active))
        .on_press(Message::CloseWindow(handle))
        .into()
    }

    fn window_tile(&self, window: &DisplayWindow, icon_size: f32) -> Element<'_, Message> {
        let text = truncate_title(&window.title, self.max_chars());
        let mut content = row![].align_y(Alignment::Center).spacing(4);

        if let Some(icon) = window.icon.clone() {
            content = content.push(
                widget::icon(icon)
                    .width(Length::Fixed(icon_size))
                    .height(Length::Fixed(icon_size)),
            );
        }

        content = content.push(self.core.applet.text(text));

        let is_active = window.is_active;
        let is_hovered = self
            .hovered_window
            .as_ref()
            .is_some_and(|hovered| hovered == &window.handle);
        let handle = window.handle.clone();
        let hover_handle = handle.clone();
        let hover_move_handle = handle.clone();
        let hover_clear_handle = handle.clone();
        let close_handle = handle.clone();
        let preview = container(content)
            .padding([2, 8])
            .class(theme::Container::custom(move |theme| {
                let cosmic = theme.cosmic();
                let (background, foreground, border_color, border_width) = if is_active {
                    (
                        if is_hovered {
                            cosmic.accent_button.hover.into()
                        } else {
                            cosmic.accent_button.base.into()
                        },
                        cosmic.accent_button.on.into(),
                        if is_hovered {
                            cosmic.accent.base.into()
                        } else {
                            iced::Color::TRANSPARENT
                        },
                        if is_hovered { 1.0 } else { 0.0 },
                    )
                } else {
                    (
                        if is_hovered {
                            cosmic.background.component.hover.into()
                        } else {
                            cosmic.background.component.base.into()
                        },
                        cosmic.background.component.on.into(),
                        if is_hovered {
                            cosmic.bg_divider().into()
                        } else {
                            iced::Color::TRANSPARENT
                        },
                        if is_hovered { 1.0 } else { 0.0 },
                    )
                };

                container::Style {
                    icon_color: Some(foreground),
                    text_color: Some(foreground),
                    background: Some(iced::Background::Color(background)),
                    border: iced::Border {
                        radius: cosmic.corner_radii.radius_s.into(),
                        color: border_color,
                        width: border_width,
                        ..Default::default()
                    },
                    shadow: Default::default(),
                    snap: true,
                }
            }));

        let close_button_overlay: Element<'_, Message> = if is_hovered {
            widget::mouse_area(
                row![
                    space::horizontal().width(Length::Fill),
                    container(Self::close_button(close_handle, is_active)).padding([0, 4])
                ]
                .align_y(Alignment::Center)
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .interaction(mouse::Interaction::Idle)
            .on_exit(Message::ClearHoveredWindow(hover_clear_handle))
            .into()
        } else {
            row![].width(Length::Fill).height(Length::Fill).into()
        };

        widget::mouse_area(stack![preview, close_button_overlay])
            .interaction(mouse::Interaction::Idle)
            .on_enter(Message::HoverWindow(hover_handle))
            .on_move(move |_| Message::HoverWindow(hover_move_handle.clone()))
            .on_exit(Message::ClearHoveredWindow(handle.clone()))
            .on_middle_press(Message::CloseWindow(handle.clone()))
            .on_press(Message::FocusWindow(handle))
            .into()
    }

    fn empty_tile(&self) -> Element<'_, Message> {
        container(self.core.applet.text(EMPTY_TITLE))
            .padding([2, 8])
            .class(theme::Container::custom(move |theme| {
                let cosmic = theme.cosmic();
                let background = cosmic.background.component.base.into();
                let foreground = cosmic.background.component.on.into();

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
                hovered_window: None,
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
            Message::ClearHoveredWindow(handle) => {
                if self
                    .hovered_window
                    .as_ref()
                    .is_some_and(|hovered| hovered == &handle)
                {
                    self.hovered_window = None;
                }
            }
            Message::ClearHoveredWindowGlobal => {
                self.hovered_window = None;
            }
            Message::CloseWindow(handle) => {
                close_window(handle);
            }
            Message::FocusWindow(handle) => {
                focus_window(handle);
            }
            Message::HoverWindow(handle) => {
                self.hovered_window = Some(handle);
            }
            Message::Wayland(update) => match update {
                WaylandUpdate::WorkspaceWindows(windows) => {
                    self.windows = windows
                        .into_iter()
                        .map(|window| DisplayWindow {
                            handle: window.handle.clone(),
                            title: window.title.clone(),
                            icon: self.resolve_icon(&window),
                            is_active: window.is_active,
                        })
                        .collect();

                    if self.hovered_window.as_ref().is_some_and(|hovered| {
                        !self.windows.iter().any(|window| &window.handle == hovered)
                    }) {
                        self.hovered_window = None;
                    }
                }
                WaylandUpdate::Finished => {
                    tracing::error!("Wayland subscription ended");
                }
            },
        }

        app::Task::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch([
            workspace_windows_subscription().map(Message::Wayland),
            event::listen_with(|event, _, _| match event {
                iced::Event::Mouse(mouse::Event::CursorLeft) => {
                    Some(Message::ClearHoveredWindowGlobal)
                }
                _ => None,
            }),
        ])
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let height = (self.core.applet.suggested_size(true).1
            + 2 * self.core.applet.suggested_padding(true).1) as f32;
        let icon_size = self.core.applet.suggested_size(true).0 as f32;
        let mut content = row![].align_y(Alignment::Center).spacing(6);

        if self.windows.is_empty() {
            content = content.push(self.empty_tile());
        } else {
            for window in &self.windows {
                content = content.push(self.window_tile(window, icon_size));
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

fn close_button_class(is_active: bool) -> theme::Button {
    theme::Button::Custom {
        active: Box::new(move |_, theme| close_button_style(theme, is_active, 0.0)),
        disabled: Box::new(move |theme| close_button_style(theme, is_active, 0.0)),
        hovered: Box::new(move |_, theme| close_button_style(theme, is_active, 0.14)),
        pressed: Box::new(move |_, theme| close_button_style(theme, is_active, 0.22)),
    }
}

fn close_button_style(
    theme: &cosmic::Theme,
    is_active: bool,
    background_alpha: f32,
) -> widget::button::Style {
    let cosmic = theme.cosmic();
    let foreground = if is_active {
        cosmic.accent_button.on.into()
    } else {
        cosmic.background.component.on.into()
    };
    let background = (background_alpha > 0.0)
        .then(|| iced::Background::Color(with_alpha(foreground, background_alpha)));

    widget::button::Style {
        shadow_offset: iced::Vector::default(),
        background,
        overlay: None,
        border_radius: cosmic.corner_radii.radius_xl.into(),
        border_width: 0.0,
        border_color: iced::Color::TRANSPARENT,
        outline_width: 0.0,
        outline_color: iced::Color::TRANSPARENT,
        icon_color: Some(foreground),
        text_color: Some(foreground),
    }
}

fn with_alpha(mut color: iced::Color, alpha: f32) -> iced::Color {
    color.a = alpha;
    color
}
