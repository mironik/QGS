use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

pub struct MonitorPreviewFrame {
    pub width: u32,
    pub height: u32,
    pub rgb8: Vec<u8>,
}

pub struct MonitorPreview {
    latest: Arc<Mutex<Option<MonitorPreviewFrame>>>,
    stop: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl MonitorPreview {
    pub fn open() -> Result<Self, Box<dyn std::error::Error>> {
        let latest = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let closed = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();
        let latest_thread = Arc::clone(&latest);
        let stop_thread = Arc::clone(&stop);
        let closed_thread = Arc::clone(&closed);
        let join = thread::Builder::new()
            .name("qgs-monitor-preview".to_string())
            .spawn(move || {
                run_window(latest_thread, stop_thread, closed_thread, ready_tx);
            })?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Self {
                latest,
                stop,
                closed,
                join: Some(join),
            }),
            Ok(Err(err)) => {
                stop.store(true, Ordering::Relaxed);
                let _ = join.join();
                Err(err.into())
            }
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                closed.store(true, Ordering::Relaxed);
                let _ = join.join();
                Err("monitor preview window did not open".into())
            }
        }
    }

    pub fn present(&self, frame: MonitorPreviewFrame) -> bool {
        if self.closed.load(Ordering::Relaxed) || self.stop.load(Ordering::Relaxed) {
            return false;
        }
        *lock_latest(&self.latest) = Some(frame);
        true
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }
}

impl Drop for MonitorPreview {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn lock_latest(
    latest: &Mutex<Option<MonitorPreviewFrame>>,
) -> std::sync::MutexGuard<'_, Option<MonitorPreviewFrame>> {
    latest.lock().unwrap_or_else(|err| err.into_inner())
}

fn run_window(
    latest: Arc<Mutex<Option<MonitorPreviewFrame>>>,
    stop: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
    ready_tx: mpsc::Sender<Result<(), String>>,
) {
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(err) => {
            closed.store(true, Ordering::Relaxed);
            let _ = ready_tx.send(Err(format!("monitor preview event loop failed: {err}")));
            return;
        }
    };
    let mut app = PreviewApp {
        latest,
        stop,
        closed,
        ready_tx: Some(ready_tx),
        window: None,
        context: None,
        surface: None,
        frame: None,
    };
    if let Err(err) = event_loop.run_app(&mut app) {
        app.closed.store(true, Ordering::Relaxed);
        if let Some(ready_tx) = app.ready_tx.take() {
            let _ = ready_tx.send(Err(format!("monitor preview window failed: {err}")));
        }
    }
    app.closed.store(true, Ordering::Relaxed);
}

struct PreviewApp {
    latest: Arc<Mutex<Option<MonitorPreviewFrame>>>,
    stop: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
    ready_tx: Option<mpsc::Sender<Result<(), String>>>,
    window: Option<Arc<Window>>,
    context: Option<softbuffer::Context<Arc<Window>>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    frame: Option<MonitorPreviewFrame>,
}

impl ApplicationHandler for PreviewApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("QGS monitor preview")
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                self.closed.store(true, Ordering::Relaxed);
                if let Some(ready_tx) = self.ready_tx.take() {
                    let _ = ready_tx.send(Err(format!(
                        "monitor preview window creation failed: {err}"
                    )));
                }
                event_loop.exit();
                return;
            }
        };
        let context = match softbuffer::Context::new(Arc::clone(&window)) {
            Ok(context) => context,
            Err(err) => {
                self.closed.store(true, Ordering::Relaxed);
                if let Some(ready_tx) = self.ready_tx.take() {
                    let _ = ready_tx.send(Err(format!("monitor preview surface failed: {err}")));
                }
                event_loop.exit();
                return;
            }
        };
        let surface = match softbuffer::Surface::new(&context, Arc::clone(&window)) {
            Ok(surface) => surface,
            Err(err) => {
                self.closed.store(true, Ordering::Relaxed);
                if let Some(ready_tx) = self.ready_tx.take() {
                    let _ = ready_tx.send(Err(format!("monitor preview surface failed: {err}")));
                }
                event_loop.exit();
                return;
            }
        };
        self.context = Some(context);
        self.surface = Some(surface);
        self.window = Some(window);
        if let Some(ready_tx) = self.ready_tx.take() {
            let _ = ready_tx.send(Ok(()));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.closed.store(true, Ordering::Relaxed);
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.stop.load(Ordering::Relaxed) {
            self.closed.store(true, Ordering::Relaxed);
            event_loop.exit();
            return;
        }
        if let Some(frame) = lock_latest(&self.latest).take() {
            self.frame = Some(frame);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
    }
}

impl PreviewApp {
    fn draw(&mut self) {
        let Some(frame) = &self.frame else {
            return;
        };
        let Some(window) = &self.window else {
            return;
        };
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        let size = window.inner_size();
        let Some(width) = NonZeroU32::new(size.width) else {
            return;
        };
        let Some(height) = NonZeroU32::new(size.height) else {
            return;
        };
        if surface.resize(width, height).is_err() {
            return;
        }
        let mut buffer = match surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(_) => return,
        };
        blit_rgb8_letterbox(
            frame.width,
            frame.height,
            &frame.rgb8,
            width.get(),
            height.get(),
            &mut buffer,
        );
        let _ = buffer.present();
    }
}

pub fn blit_rgb8_letterbox(
    src_width: u32,
    src_height: u32,
    rgb8: &[u8],
    dst_width: u32,
    dst_height: u32,
    buffer: &mut [u32],
) {
    buffer.fill(0);
    if src_width == 0 || src_height == 0 || dst_width == 0 || dst_height == 0 {
        return;
    }
    let expected = (src_width as usize)
        .saturating_mul(src_height as usize)
        .saturating_mul(3);
    if rgb8.len() < expected {
        return;
    }
    let scale = (dst_width as f64 / src_width as f64).min(dst_height as f64 / src_height as f64);
    let draw_width = ((src_width as f64) * scale).floor().max(1.0) as u32;
    let draw_height = ((src_height as f64) * scale).floor().max(1.0) as u32;
    let draw_width = draw_width.min(dst_width);
    let draw_height = draw_height.min(dst_height);
    let origin_x = (dst_width - draw_width) / 2;
    let origin_y = (dst_height - draw_height) / 2;
    for y in 0..draw_height {
        let src_y = (u64::from(y) * u64::from(src_height)) / u64::from(draw_height);
        for x in 0..draw_width {
            let src_x = (u64::from(x) * u64::from(src_width)) / u64::from(draw_width);
            let src_index = ((src_y * u64::from(src_width) + src_x) * 3) as usize;
            let red = u32::from(rgb8[src_index]);
            let green = u32::from(rgb8[src_index + 1]);
            let blue = u32::from(rgb8[src_index + 2]);
            let dst_x = origin_x + x;
            let dst_y = origin_y + y;
            let dst_index = (dst_y * dst_width + dst_x) as usize;
            buffer[dst_index] = (red << 16) | (green << 8) | blue;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::blit_rgb8_letterbox;

    #[test]
    fn letterbox_scales_source_pixels_into_the_center() {
        let rgb8 = vec![
            255, 0, 0, //
            0, 255, 0, //
            0, 0, 255, //
            255, 255, 255,
        ];
        let mut buffer = vec![1_u32; 24];
        blit_rgb8_letterbox(2, 2, &rgb8, 6, 4, &mut buffer);
        assert_eq!(buffer[0], 0);
        assert_eq!(buffer[1], 255 << 16);
        assert_eq!(buffer[2], 255 << 16);
        assert_eq!(buffer[3], 255 << 8);
        assert_eq!(buffer[4], 255 << 8);
        assert_eq!(buffer[5], 0);
        assert_eq!(buffer[13], 255);
        assert_eq!(buffer[15], (255 << 16) | (255 << 8) | 255);
    }
}
