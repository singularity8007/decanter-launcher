//! The window, built with iced 0.14 (Elm-style: state, messages, update, view).
//!
//! It draws what `Backend::view()` says and forwards button clicks to
//! `Backend`. It never touches downloads, threads or the game, so swapping
//! this file for a different one changes nothing else.

use crate::backend::{Backend, Notice, View};
use crate::settings::{Field, Toggle};
use iced::time::{self, milliseconds};
use iced::widget::{
    Column, button, center, column, container, mouse_area, opaque, progress_bar, row, space,
    scrollable, stack, text, text_input, toggler,
};
use iced::{
    Background, Center, Color, Degrees, Element, Fill, Font, Length, Padding, Shadow, Size,
    Subscription, Task, Theme, Vector, border, color, font, gradient, window,
};

// ---------- Colours: near-black, one white accent ----------

const INK: Color = color!(0xf2f2f5); // main text and the PLAY button 0xf2f2f5
const DIM: Color = color!(0x8a8a94); // small labels 0x8a8a94
const LINE: Color = color!(0x25252b); // hairline borders 0x25252b
const LINE_HOVER: Color = color!(0x6a6a76); // 0x6a6a76
const SURFACE: Color = color!(0x0e0e11); // cards and the popup 0x0e0e11
const GOOD: Color = color!(0x9fd8b8); // good news text 0x9fd8b8
const WARN: Color = color!(0xf28b8b); // error popup title 0xf28b8b
const DARK_RED: Color = color!(0xc04a4a); // settings notice 0xc04a4a

pub fn run(backend: Backend) -> iced::Result {
    iced::application(move || App::new(backend.clone()), App::update, App::view)
        .title("wine_launcher")
        .subscription(App::subscription)
        .theme(App::theme)
        .window_size(Size::new(940.0, 600.0))
        .centered()
        .run()
}

struct App {
    backend: Backend,
    theme: Theme,
    page: Page,
    /// What's typed in each box on the settings page. Saved as the player types.
    paths: Vec<(Field, String)>,
    /// Launch switches as shown on the settings page.
    flags: Vec<(Toggle, bool)>,
    /// FPS cap box ("" = no cap).
    fps_cap: String,
}

/// Which screen the window is showing.
enum Page {
    Home,
    Settings,
}

#[derive(Debug, Clone)]
enum Message {
    Launch,
    Install,
    CheckUpdates,
    DismissNotice,
    OpenSettings,
    CloseSettings,
    PathChanged(Field, String),
    FlagChanged(Toggle, bool),
    FpsCapChanged(String),
    Tick,
}

impl App {
    fn new(backend: Backend) -> Self {
        let palette = iced::theme::Palette {
            background: color!(0x050506),
            text: INK,
            primary: INK,
            success: GOOD,
            warning: color!(0xfbbf24),
            danger: WARN,
        };
        let paths: Vec<(Field, String)> = Field::ALL
            .iter()
            .map(|&field| (field, backend.path_in_use(field)))
            .collect();
        let flags = Toggle::ALL
            .iter()
            .map(|&t| (t, backend.flag_in_use(t)))
            .collect();
        let fps_cap = backend.fps_cap_in_use();
        Self {
            flags,
            fps_cap,
            backend,
            theme: Theme::custom("wine_launcher", palette),
            page: Page::Home,
            paths,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Launch => {
                self.backend.launch();
                // Get out of the game's way, like Twintail does (one less
                // surface for the compositor while playing).
                return window::latest()
                    .and_then(|id| window::minimize(id, true));
            }
            Message::Install => self.backend.install_game(),
            Message::CheckUpdates => self.backend.check_updates(),
            Message::DismissNotice => self.backend.dismiss(),
            Message::OpenSettings => self.page = Page::Settings,
            Message::CloseSettings => self.page = Page::Home,
            Message::PathChanged(field, typed) => {
                self.backend.set_path(field, &typed);
                if let Some(entry) = self.paths.iter_mut().find(|(f, _)| *f == field) {
                    entry.1 = typed;
                }
            }
            Message::FlagChanged(toggle, on) => {
                self.backend.set_flag(toggle, on);
                if let Some(entry) = self.flags.iter_mut().find(|(t, _)| *t == toggle) {
                    entry.1 = on;
                }
            }
            Message::FpsCapChanged(typed) => {
                // Only digits make it into the box.
                if typed.chars().all(|c| c.is_ascii_digit()) {
                    self.backend.set_fps_cap(&typed);
                    self.fps_cap = typed;
                }
            }
            // Nothing to do with the time itself: any message makes iced redraw,
            // which re-reads the backend. The backend just saves play time.
            Message::Tick => self.backend.tick(),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        match self.page {
            Page::Home => self.home(),
            Page::Settings => self.settings(),
        }
    }

    /// The main page: header, status and the buttons.
    fn home(&self) -> Element<'_, Message> {
        let view = self.backend.view();

        let page = container(
            column![
                header(&view),
                space::vertical(),
                status(&view),
                row![actions(view.busy), space::horizontal(), settings_link()].align_y(Center),
            ]
            .spacing(24),
        )
        .padding(32)
        .width(Fill)
        .height(Fill)
        .style(page_style);

        // Errors pop up over the page; the page is still there underneath.
        match view.notice.filter(|notice| notice.is_error) {
            Some(notice) => modal(page.into(), popup(&notice)),
            None => page.into(),
        }
    }

    /// The settings page: one box per path, each with its label above it.
    fn settings(&self) -> Element<'_, Message> {
        let notice = text("Close the game launcher and start it again to save the changes.")
            .size(13)
            .color(DARK_RED);

        let mut fields = Column::new().spacing(14);

        // Launch switches first: these are the ones that change how the game runs.
        for (toggle, on) in &self.flags {
            let toggle = *toggle;
            fields = fields.push(
                toggler(*on)
                    .label(toggle.label())
                    .text_size(13)
                    .on_toggle(move |on| Message::FlagChanged(toggle, on)),
            );
        }
        fields = fields.push(
            column![
                text("FPS cap (empty = none). With FreeSync on, ~138 on a 144 Hz screen; without it, 72 or 144.")
                    .size(11)
                    .font(bold())
                    .color(DIM),
                text_input("none", &self.fps_cap)
                    .on_input(Message::FpsCapChanged)
                    .padding(10)
                    .size(13)
                    .width(Length::Fixed(160.0))
                    .style(input_style),
            ]
            .spacing(6),
        );
        for (field, value) in &self.paths {
            let mut input = text_input("Path", value)
                .padding(10)
                .size(13)
                .style(input_style);
            // A box with no `on_input` can't be typed in.
            if field.editable() {
                let field = *field;
                input = input.on_input(move |typed| Message::PathChanged(field, typed));
            }
            fields = fields.push(
                column![
                    text(field.label()).size(11).font(bold()).color(DIM),
                    input,
                ]
                .spacing(6),
            );
        }

        // The list scrolls because there are more boxes than the window is tall.
        let list = scrollable(container(fields).padding(Padding {
            right: 16.0,
            ..Padding::ZERO
        }))
        .height(Fill);

        // Back sits in the bottom-right corner, where the gear was.
        let back = button(text("← Back").size(13))
            .on_press(Message::CloseSettings)
            .padding([6, 10])
            .style(link_button);

        container(column![notice, list, row![space::horizontal(), back]].spacing(24))
            .padding(32)
            .width(Fill)
            .height(Fill)
            .style(page_style)
            .into()
    }

    /// A steady tick so the progress bar and play time stay current. Slower
    /// while nothing is happening.
    fn subscription(&self) -> Subscription<Message> {
        let every = if self.backend.is_busy() {
            milliseconds(250)
        } else {
            milliseconds(1000)
        };
        time::every(every).map(|_| Message::Tick)
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }
}

// ---------- Layout ----------

fn header<'a>(view: &View) -> Element<'a, Message> {
    let title = column![
        text("ZENLESS ZONE ZERO").size(28).font(bold()),
        text("A product of the wine launcher product NAME TO BE DECIDED").size(13).color(DIM),
    ]
    .spacing(4);

    let stats = container(
        row![
            stat("PLAY TIME", view.play_time.clone()),
            stat("LAST PLAYED", view.last_played.clone()),
        ]
        .spacing(32),
    )
    .padding([12, 22])
    .style(card_style);

    row![title, space::horizontal(), stats].align_y(Center).into()
}

fn stat<'a>(label: &'static str, value: String) -> Column<'a, Message> {
    column![
        text(label).size(11).font(bold()).color(DIM),
        text(value).size(17).font(bold()),
    ]
    .spacing(3)
}

/// The progress bar while a job runs, or the last good-news message.
fn status<'a>(view: &View) -> Element<'a, Message> {
    if view.busy {
        let mut body = Column::new().spacing(10).push(text(view.label.clone()).size(14).color(DIM));
        if let Some(fraction) = view.fraction {
            body = body.push(progress_bar(0.0..=1.0, fraction).girth(4.0).style(bar_style));
        }
        return body.into();
    }

    match &view.notice {
        Some(notice) if !notice.is_error => {
            text(notice.text.clone()).size(14).color(GOOD).into()
        }
        _ => space().into(),
    }
}

fn actions<'a>(busy: bool) -> Element<'a, Message> {
    // `None` greys a button out and makes it ignore clicks.
    let when_free = |message: Message| (!busy).then_some(message);

    let play = button(text("Play").size(20).font(bold()).width(Fill).center())
        .on_press_maybe(when_free(Message::Launch))
        .width(Length::Fixed(240.0))
        .padding(15)
        .style(play_button);

    let install = button(text("Install/Update ZZZ").size(14).center())
        .on_press_maybe(when_free(Message::Install))
        .padding([14, 24])
        .style(secondary_button);

    let check = button(text("Check for updates").size(14).center())
        .on_press_maybe(when_free(Message::CheckUpdates))
        .padding([14, 24])
        .style(secondary_button);

    row![play, install, check].spacing(12).align_y(Center).into()
}

/// The small gear in the bottom-right corner that opens the settings page.
fn settings_link<'a>() -> Element<'a, Message> {
    button(
        row![
            text("⚙").size(18).color(INK),
            text("Settings").size(12).color(DIM),
        ]
        .spacing(8)
        .align_y(Center),
    )
    .on_press(Message::OpenSettings)
    .padding([6, 10])
    .style(link_button)
    .into()
}

/// The error popup: a small card with a small x in the corner.
fn popup<'a>(notice: &Notice) -> Element<'a, Message> {
    let close = button(text("×").size(18).width(Fill).center())
        .on_press(Message::DismissNotice)
        .width(Length::Fixed(26.0))
        .height(Length::Fixed(26.0))
        .padding(0)
        .style(close_button);

    let mut body = Column::new()
        .spacing(12)
        .push(
            row![
                text("Heads up").size(17).font(bold()).color(WARN),
                space::horizontal(),
                close
            ]
            .align_y(Center),
        )
        .push(text(notice.text.clone()).size(14));

    if let Some(details) = &notice.details {
        body = body.push(
            text(format!("Technical details: {details}"))
                .size(11)
                .color(DIM),
        );
    }

    container(body)
        .width(Length::Fixed(400.0))
        .padding(18)
        .style(popup_style)
        .into()
}

/// Dim the page and show `content` on top. Clicking outside the card closes it.
fn modal<'a>(base: Element<'a, Message>, content: Element<'a, Message>) -> Element<'a, Message> {
    stack![
        base,
        opaque(
            mouse_area(center(opaque(content)).style(|_theme| container::Style {
                background: Some(
                    Color {
                        a: 0.75,
                        ..Color::BLACK
                    }
                    .into()
                ),
                ..container::Style::default()
            }))
            .on_press(Message::DismissNotice)
        )
    ]
    .into()
}

// ---------- Look ----------

fn bold() -> Font {
    Font {
        weight: font::Weight::Bold,
        ..Font::DEFAULT
    }
}

/// Almost-black, with a barely-there lift at the top.
fn page_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::from(
            gradient::Linear::new(Degrees(160.0))
                .add_stop(0.0, color!(0x0d0d11))
                .add_stop(1.0, color!(0x040405)),
        )),
        ..container::Style::default()
    }
}

fn card_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        border: border::rounded(12.0).color(LINE).width(1.0),
        ..container::Style::default()
    }
}

fn popup_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        border: border::rounded(14.0).color(color!(0x34343c)).width(1.0),
        shadow: Shadow {
            color: Color {
                a: 0.8,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 10.0),
            blur_radius: 30.0,
        },
        ..container::Style::default()
    }
}

/// A thin white line that fills as the job progresses.
fn bar_style(_theme: &Theme) -> progress_bar::Style {
    progress_bar::Style {
        background: color!(0x1a1a1f).into(),
        bar: INK.into(),
        border: border::rounded(2.0),
    }
}

/// The big solid white PLAY button.
fn play_button(_theme: &Theme, status: button::Status) -> button::Style {
    let (background, text_color) = match status {
        button::Status::Active => (INK, Color::BLACK),
        button::Status::Hovered => (Color::WHITE, Color::BLACK),
        button::Status::Pressed => (color!(0xc4c4cc), Color::BLACK),
        button::Status::Disabled => (color!(0x1a1a1f), color!(0x55555e)),
    };
    let glow = matches!(status, button::Status::Hovered);

    button::Style {
        background: Some(background.into()),
        text_color,
        border: border::rounded(10.0),
        shadow: if glow {
            Shadow {
                color: Color { a: 0.2, ..Color::WHITE },
                offset: Vector::new(0.0, 0.0),
                blur_radius: 24.0,
            }
        } else {
            Shadow::default()
        },
        snap: false,
    }
}

/// The quiet outlined buttons next to PLAY.
fn secondary_button(_theme: &Theme, status: button::Status) -> button::Style {
    let (background, outline, text_color) = match status {
        button::Status::Active => (Color::TRANSPARENT, LINE, color!(0xd8d8de)),
        button::Status::Hovered => (color!(0x121216), LINE_HOVER, Color::WHITE),
        button::Status::Pressed => (color!(0x0a0a0c), LINE_HOVER, color!(0xd8d8de)),
        button::Status::Disabled => (Color::TRANSPARENT, color!(0x18181c), color!(0x4a4a52)),
    };

    button::Style {
        background: Some(background.into()),
        text_color,
        border: border::rounded(10.0).color(outline).width(1.0),
        shadow: Shadow::default(),
        snap: false,
    }
}

/// The small x on the error popup.
fn close_button(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => color!(0x26262d),
        _ => Color::TRANSPARENT,
    };

    button::Style {
        background: Some(background.into()),
        text_color: color!(0xb0b0ba),
        border: border::rounded(8.0),
        shadow: Shadow::default(),
        snap: false,
    }
}

/// Quiet buttons with no outline: the settings gear and the back button.
fn link_button(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => color!(0x121216),
        _ => Color::TRANSPARENT,
    };

    button::Style {
        background: Some(background.into()),
        text_color: DIM,
        border: border::rounded(8.0),
        shadow: Shadow::default(),
        snap: false,
    }
}

/// The text box on the settings page: dark card, hairline outline that
/// brightens when the box is hovered or focused.
fn input_style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let outline = match status {
        text_input::Status::Active | text_input::Status::Disabled => LINE,
        _ => LINE_HOVER,
    };

    text_input::Style {
        background: SURFACE.into(),
        border: border::rounded(10.0).color(outline).width(1.0),
        value: if matches!(status, text_input::Status::Disabled) { DIM } else { INK },
        ..text_input::default(theme, status)
    }
}