use super::items::{Item, MouseEvent};
use std::rc::Rc;
use swsurface::{Format, SwWindow};


/// Use as a factory for RSMLItem
pub trait ItemFactory {
    fn create() -> Rc<dyn Item<'static>>;
}

pub fn show_window<T: ItemFactory + 'static>() {

    use winit::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
        window::WindowBuilder,
    };

    let item = T::create();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();


    let event_loop_proxy = event_loop.create_proxy();
    let sw_context = swsurface::ContextBuilder::new(&event_loop)
        .with_ready_cb(move |_| {
            let _ = event_loop_proxy.send_event(());
        })
        .build();

    let sw_window = SwWindow::new(window, &sw_context, &Default::default());

    let format = [Format::Xrgb8888, Format::Argb8888]
        .iter()
        .cloned()
        .find(|&fmt1| sw_window.supported_formats().any(|fmt2| fmt1 == fmt2))
        .unwrap();

    sw_window.update_surface_to_fit(format);
    sw_window.window().request_redraw();

    let mut cursor_pos = piet_common::kurbo::Point::ORIGIN;

    event_loop.run(move |event, _, control_flow| {
        match event {

            Event::WindowEvent { event: WindowEvent::Resized(_), .. } |
            Event::WindowEvent { event: WindowEvent::HiDpiFactorChanged(_), ..} => {
                sw_window.update_surface_to_fit(format);
                let [width, height] = sw_window.image_info().extent;
                item.geometry().width.set(width.into());
                item.geometry().height.set(height.into());
                redraw(&sw_window, item.clone());
            }

            Event::EventsCleared => {
                // Application update code.

                // Queue a RedrawRequested event.
                //sw_window.window().request_redraw();
            },
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                // Redraw the application.
                //
                // It's preferrable to render in this event rather than in EventsCleared, since
                // rendering in here allows the program to gracefully handle redraws requested
                // by the OS.
                redraw(&sw_window, item.clone());
            },
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                println!("The close button was pressed; stopping");
                *control_flow = ControlFlow::Exit
            },
            Event::WindowEvent {
                event: WindowEvent::CursorMoved{ position, .. },
                ..
            } => {
                let f = sw_window.window().hidpi_factor();
                cursor_pos = piet_common::kurbo::Point::new(position.x*f, position.y*f);
                item.mouse_event(MouseEvent::Move(cursor_pos));
            },
            Event::WindowEvent {
                event: WindowEvent::MouseInput{ state, .. },
                ..
            } => {
                item.mouse_event(match state {
                    winit::event::ElementState::Pressed => MouseEvent::Press(cursor_pos),
                    winit::event::ElementState::Released => MouseEvent::Release(cursor_pos),
                });
                // FIXME: listen on property changes
                sw_window.window().request_redraw();
            }
            // ControlFlow::Poll continuously runs the event loop, even if the OS hasn't
            // dispatched any events. This is ideal for games and similar applications.
            //_ => *control_flow = ControlFlow::Poll,
            // ControlFlow::Wait pauses the event loop if no events are available to process.
            // This is ideal for non-game applications that only update in response to user
            // input, and uses significantly less power/CPU time than ControlFlow::Poll.
            _ => *control_flow = ControlFlow::Wait,
        }
    });
}

fn redraw(sw_window: &SwWindow, item: Rc<dyn Item>) {

    use piet_common::{ImageFormat, RenderContext, Color};
    use piet_common::Device;

    if let Some(image_index) = sw_window.poll_next_image() {
        let device = Device::new().unwrap();
        let swsurface::ImageInfo { extent: [width, height], stride, .. } = sw_window.image_info();
        let mut bitmap = device.bitmap_target(width as usize, height as usize, 1.0).unwrap();
        let mut rc = bitmap.render_context();
        rc.clear(Color::WHITE);
        item.draw(&mut rc).unwrap();
        // FIXME!  This is too slow. can't we directly paint on the surface?
        let raw_pixels = bitmap.into_raw_pixels(ImageFormat::RgbaPremul).unwrap();
        {
            let mut surface = sw_window.lock_image(image_index);
            for y in 0..(height as usize) {
                (*surface)[y*stride..(y*stride + (4*width as usize))].copy_from_slice(&raw_pixels[y*(width as usize)*4..(y+1)*(width as usize)*4]);
            }
        }
        sw_window.present_image(image_index);
    }
}

