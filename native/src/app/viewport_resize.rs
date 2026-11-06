use egui::{Color32, CursorIcon, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

#[derive(Default)]
pub(super) struct ViewportResize {
    drag: Option<Drag>,
}

struct Drag {
    origin: Pos2,
    rect: Rect,
    viewport: [u32; 2],
    edge: [i8; 2],
    pending: [u32; 2],
}

#[derive(Default)]
pub(super) struct ResizeResponse {
    pub captured: bool,
    pub viewport: Option<[u32; 2]>,
}

impl Drag {
    fn update(&mut self, pointer: Pos2, maximum: u32) -> Rect {
        let delta = pointer - self.origin;
        let pixels_per_point =
            Vec2::new(self.viewport[0] as f32, self.viewport[1] as f32) / self.rect.size();
        let mut rect = self.rect;
        for axis in 0..2 {
            if self.edge[axis] == 0 {
                continue;
            }
            self.pending[axis] = (self.viewport[axis] as f32
                + delta[axis] * pixels_per_point[axis] * self.edge[axis] as f32)
                .round()
                .clamp(64.0, maximum as f32) as u32;
            let size = self.pending[axis] as f32 / pixels_per_point[axis];
            if self.edge[axis] < 0 {
                rect.min[axis] = rect.max[axis] - size;
            } else {
                rect.max[axis] = rect.min[axis] + size;
            }
        }
        rect
    }
}

impl ViewportResize {
    pub fn clear(&mut self) {
        self.drag = None;
    }

    /// The outline follows the pointer at the scale where the drag began. Commit on release so
    /// fitting the newly rendered viewport cannot move the edge out from under the pointer.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        viewport: [u32; 2],
        maximum: u32,
    ) -> ResizeResponse {
        let mut result = ResizeResponse::default();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return result;
        }
        let radius = 6.0;
        let mut hovered = false;
        // Corners are separate from the edge strips, so both dimensions can be dragged at once.
        for edge in [
            [-1, 0],
            [1, 0],
            [0, -1],
            [0, 1],
            [-1, -1],
            [1, -1],
            [-1, 1],
            [1, 1],
        ] {
            let center = Pos2::new(
                if edge[0] < 0 {
                    rect.left()
                } else if edge[0] > 0 {
                    rect.right()
                } else {
                    rect.center().x
                },
                if edge[1] < 0 {
                    rect.top()
                } else if edge[1] > 0 {
                    rect.bottom()
                } else {
                    rect.center().y
                },
            );
            let size = Vec2::new(
                if edge[0] == 0 {
                    (rect.width() - radius * 2.0).max(0.0)
                } else {
                    radius * 2.0
                },
                if edge[1] == 0 {
                    (rect.height() - radius * 2.0).max(0.0)
                } else {
                    radius * 2.0
                },
            );
            let cursor = match edge {
                [0, _] => CursorIcon::ResizeVertical,
                [_, 0] => CursorIcon::ResizeHorizontal,
                [-1, -1] | [1, 1] => CursorIcon::ResizeNwSe,
                _ => CursorIcon::ResizeNeSw,
            };
            let response = ui
                .interact(
                    Rect::from_center_size(center, size),
                    ui.id().with(("viewport-edge", edge)),
                    Sense::drag(),
                )
                .on_hover_cursor(cursor);
            hovered |= response.hovered();
            if response.drag_started() {
                let origin = ui
                    .input(|input| input.pointer.press_origin())
                    .unwrap_or(center);
                self.drag = Some(Drag {
                    origin,
                    rect,
                    viewport,
                    edge,
                    pending: viewport,
                });
            }
        }
        result.captured = hovered || self.drag.is_some();
        let mut outline = rect;
        if let Some(drag) = &mut self.drag {
            ui.ctx().set_cursor_icon(match drag.edge {
                [0, _] => CursorIcon::ResizeVertical,
                [_, 0] => CursorIcon::ResizeHorizontal,
                [-1, -1] | [1, 1] => CursorIcon::ResizeNwSe,
                _ => CursorIcon::ResizeNeSw,
            });
            if let Some(pointer) = ui.input(|input| input.pointer.latest_pos()) {
                outline = drag.update(pointer, maximum);
            }
            let text = format!(
                "{} × {} · release to resize · Esc to cancel",
                drag.pending[0], drag.pending[1]
            );
            let label =
                ui.painter()
                    .layout_no_wrap(text, egui::FontId::proportional(12.0), Color32::WHITE);
            let label_rect = Rect::from_min_size(
                ui.clip_rect().min + Vec2::splat(8.0),
                label.size() + Vec2::splat(8.0),
            );
            ui.painter()
                .rect_filled(label_rect, 3.0, Color32::from_black_alpha(230));
            ui.painter()
                .galley(label_rect.min + Vec2::splat(4.0), label, Color32::WHITE);
            if !ui.input(|input| input.focused)
                || ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                self.drag = None;
            } else if ui.input(|input| input.pointer.any_released()) {
                result.viewport = (drag.pending != drag.viewport).then_some(drag.pending);
                self.drag = None;
            }
        }
        let color = if result.captured {
            ui.visuals().selection.stroke.color
        } else {
            Color32::from_gray(100)
        };
        ui.painter()
            .rect_stroke(outline, 0.0, Stroke::new(1.0, color), StrokeKind::Inside);
        for point in [
            outline.left_top(),
            outline.right_top(),
            outline.left_bottom(),
            outline.right_bottom(),
        ] {
            ui.painter()
                .rect_filled(Rect::from_center_size(point, Vec2::splat(6.0)), 1.0, color);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        resize: &mut ViewportResize,
        events: Vec<egui::Event>,
    ) -> ResizeResponse {
        let mut response = ResizeResponse::default();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 800.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let rect =
                        Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(600.0, 450.0));
                    ui.put(
                        rect,
                        egui::Image::new((egui::TextureId::User(1), rect.size()))
                            .sense(Sense::click_and_drag()),
                    );
                    response = resize.show(
                        ui,
                        Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(600.0, 450.0)),
                        [1200, 900],
                        8192,
                    );
                });
            },
        );
        response
    }

    fn button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn pointer_drag_commits_only_on_release_and_escape_cancels() {
        let ctx = egui::Context::default();
        let mut resize = ViewportResize::default();
        let start = Pos2::new(700.0, 550.0);
        let end = Pos2::new(750.0, 575.0);
        frame(&ctx, &mut resize, vec![]);
        frame(&ctx, &mut resize, vec![egui::Event::PointerMoved(start)]);
        frame(&ctx, &mut resize, vec![button(start, true)]);
        let moving = frame(&ctx, &mut resize, vec![egui::Event::PointerMoved(end)]);
        assert!(moving.captured);
        assert_eq!(moving.viewport, None);
        assert_eq!(
            frame(&ctx, &mut resize, vec![button(end, false)]).viewport,
            Some([1300, 950])
        );
        assert!(resize.drag.is_none());

        frame(&ctx, &mut resize, vec![egui::Event::PointerMoved(start)]);
        frame(&ctx, &mut resize, vec![button(start, true)]);
        frame(&ctx, &mut resize, vec![egui::Event::PointerMoved(end)]);
        let cancel = frame(
            &ctx,
            &mut resize,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(cancel.viewport, None);
        assert!(resize.drag.is_none());
        assert_eq!(
            frame(&ctx, &mut resize, vec![button(end, false)]).viewport,
            None
        );
    }

    #[test]
    fn edges_resize_logical_pixels_at_the_original_display_scale() {
        let rect = Rect::from_min_size(Pos2::new(100.0, 80.0), Vec2::new(600.0, 450.0));
        for edge in [
            [-1, 0],
            [1, 0],
            [0, -1],
            [0, 1],
            [-1, -1],
            [1, -1],
            [-1, 1],
            [1, 1],
        ] {
            let mut drag = Drag {
                origin: rect.center(),
                rect,
                viewport: [1200, 900],
                edge,
                pending: [1200, 900],
            };
            let resized = drag.update(rect.center() + Vec2::new(50.0, 25.0), 8192);
            assert_eq!(
                drag.pending,
                [
                    (1200 + 100 * edge[0] as i32) as u32,
                    (900 + 50 * edge[1] as i32) as u32
                ]
            );
            for axis in 0..2 {
                if edge[axis] < 0 {
                    assert_eq!(resized.max[axis], rect.max[axis]);
                } else {
                    assert_eq!(resized.min[axis], rect.min[axis]);
                }
            }
        }
    }

    #[test]
    fn dragging_back_does_not_accumulate_rounding_and_respects_texture_limits() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0));
        let mut drag = Drag {
            origin: Pos2::ZERO,
            rect,
            viewport: [1200, 900],
            edge: [1, 1],
            pending: [1200, 900],
        };
        drag.update(Pos2::new(10000.0, -10000.0), 4000);
        assert_eq!(drag.pending, [4000, 64]);
        assert_eq!(drag.update(Pos2::ZERO, 4000), rect);
        assert_eq!(drag.pending, [1200, 900]);
    }
}
