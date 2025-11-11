use crate::events::NotificationEvent;
use eframe::{
    NativeOptions,
    egui::{self, Color32, ColorImage, TextureHandle, TextureOptions, ViewportBuilder},
};
use image;
use rust_embed::RustEmbed;
use std::sync::mpsc::Receiver;

#[derive(RustEmbed)]
#[folder = "images/"]
struct Assets;

pub fn run_gui(image_receiver: Receiver<NotificationEvent>) -> Result<(), eframe::Error> {
    let options = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_transparent(true)
            .with_decorations(false)
            .with_maximized(true)
            .with_always_on_top()
            .with_mouse_passthrough(true),
        ..Default::default()
    };

    eframe::run_native(
        "Reposouls Notification",
        options,
        Box::new(move |cc| Box::new(App::new(cc, image_receiver))),
    )
}

#[derive(PartialEq)]
enum AppState {
    Idle,
    FadingIn,
    Displaying,
    FadingOut,
}

struct App {
    image_receiver: Receiver<NotificationEvent>,
    texture: Option<TextureHandle>,
    state: AppState,
    animation_time: f64,
}

impl App {
    fn new(_cc: &eframe::CreationContext<'_>, image_receiver: Receiver<NotificationEvent>) -> Self {
        Self {
            image_receiver,
            texture: None,
            state: AppState::Idle,
            animation_time: 0.0,
        }
    }

    fn get_image_path_for_event(event: &NotificationEvent) -> &'static str {
        match event {
            NotificationEvent::CiSuccess => "CI PIPELINE GREENED.png",
            NotificationEvent::CiFailure => "CI PIPELINE FAILED.png",
            NotificationEvent::PrApproved => "PR APPROVAL GRANTED.png",
            NotificationEvent::PrChangesRequested => "PR CHANGES REQUIRED.png",
            NotificationEvent::PrMerged => "PR MERGE COMPLETED.png",
            NotificationEvent::PrNewComment => "PR NEW COMMENT APPEARED.png",
        }
    }

    fn load_texture(&mut self, image_path: &str, ctx: &egui::Context) {
        if let Some(asset) = Assets::get(image_path) {
            if let Ok(decoded) = image::load_from_memory(&asset.data) {
                let image = decoded.to_rgba8();
                let (width, height) = image.dimensions();
                let image_data = image.into_raw();
                let color_image = ColorImage::from_rgba_unmultiplied(
                    [width as usize, height as usize],
                    &image_data,
                );
                self.texture =
                    Some(ctx.load_texture(image_path, color_image, TextureOptions::default()));
            } else {
                eprintln!("Failed to decode embedded image: {}", image_path);
            }
        } else {
            eprintln!("Failed to find embedded image: {}", image_path);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(event) = self.image_receiver.try_recv() {
            println!("GUI: Received event to display: {:?}", event);
            let image_path = App::get_image_path_for_event(&event);
            self.load_texture(image_path, ctx);
            self.state = AppState::FadingIn;
            self.animation_time = 0.0;
        }

        self.animation_time += ctx.input(|i| i.unstable_dt) as f64;

        let opacity = match self.state {
            AppState::Idle => 0.0,
            AppState::FadingIn => {
                if self.animation_time >= 0.5 {
                    self.state = AppState::Displaying;
                    self.animation_time = 0.0;
                    1.0
                } else {
                    self.animation_time / 0.5
                }
            }
            AppState::Displaying => {
                if self.animation_time < 2.0 {
                    1.0
                } else {
                    if !ctx.input(|i| i.events.is_empty()) {
                        self.state = AppState::FadingOut;
                        self.animation_time = 0.0;
                    }
                    1.0
                }
            }
            AppState::FadingOut => {
                if self.animation_time >= 0.5 {
                    self.state = AppState::Idle;
                    self.texture = None;
                    0.0
                } else {
                    1.0 - (self.animation_time / 0.5)
                }
            }
        };

        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(opacity < 0.1));

        if let Some(texture) = &self.texture {
            let final_opacity = (opacity.clamp(0.0, 1.0) * 255.0) as u8;
            let screen_rect = ctx.screen_rect();
            let center = screen_rect.center();
            let image_pos = center - texture.size_vec2() / 2.0;

            egui::Area::new("notification_area".into())
                .fixed_pos(image_pos)
                .show(ctx, |ui| {
                    ui.add(
                        egui::Image::new(texture).tint(Color32::from_rgba_unmultiplied(
                            255,
                            255,
                            255,
                            final_opacity,
                        )),
                    );
                });
        }

        ctx.request_repaint();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}
