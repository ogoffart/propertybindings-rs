use super::items::{Item, MouseEvent};
use std::rc::Rc;

use winit::event_loop::EventLoop;
use winit::event_loop::ControlFlow;
use winit::window::Window;
use winit::event::{Event, WindowEvent};

enum UserEvent { ReadyCB, User(Box<dyn FnOnce() + Send>) }


pub struct Application {
    event_loop: EventLoop<UserEvent>,
}

impl Default for Application {
    fn default() -> Application {
        Application{ event_loop: EventLoop::with_user_event() }
    }
}



/// Use as a factory for RSMLItem
pub trait ItemFactory {
    fn create() -> Rc<dyn Item<'static>>;
    fn tick() {}
}

struct ApplicationState {
    cursor_pos: piet_common::kurbo::Point
}


// process the event, return true if one should draw
fn process_event<T: ItemFactory>(app_state : &mut ApplicationState, item : &dyn Item, window: &Window, event: Event<UserEvent>,
        control_flow: &mut ControlFlow) -> bool {
    use winit::event::*;
    match event {
        Event::EventsCleared => {
            // Application update code.

            // Queue a RedrawRequested event.
            T::tick();
            window.request_redraw();
            *control_flow = ControlFlow::WaitUntil(instant::Instant::now() + instant::Duration::from_millis(16));
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
            return true;
        },
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            //The close button was pressed; stopping;
            *control_flow = ControlFlow::Exit
        },
        Event::WindowEvent {
            event: WindowEvent::CursorMoved{ position, .. },
            ..
        } => {
            let f = window.hidpi_factor();
            app_state.cursor_pos = piet_common::kurbo::Point::new(position.x*f, position.y*f);
            item.mouse_event(MouseEvent::Move(app_state.cursor_pos));
        },
        Event::WindowEvent {
            event: WindowEvent::MouseInput{ state, .. },
            ..
        } => {
            item.mouse_event(match state {
                winit::event::ElementState::Pressed => MouseEvent::Press(app_state.cursor_pos),
                winit::event::ElementState::Released => MouseEvent::Release(app_state.cursor_pos),
            });
            // FIXME: listen on property changes
            window.request_redraw();
        }
        Event::UserEvent(UserEvent::User(callback)) => {
            callback();
        }
        Event::UserEvent(UserEvent::ReadyCB) => {
            return true;
        }
        _ => {},
    };

    false
}


impl Application {

    #[cfg(not(target_arch="wasm32"))]
    pub fn get_callback<F : FnOnce() + Send + 'static>(&self) -> impl Fn(F) + Send {
        let proxy = self.event_loop.create_proxy();
        move |callback| { proxy.send_event(UserEvent::User(Box::new(callback))).unwrap_or_else(|_| panic!("Could not send the event") );  }
    }

    #[cfg(target_arch="wasm32")]
    pub fn get_callback<F : FnOnce() + Send + 'static>(&self) -> impl Fn(F) {
        let proxy = self.event_loop.create_proxy();
        move |callback| { proxy.send_event(UserEvent::User(Box::new(callback))).unwrap_or_else(|_| panic!("Could not send the event") );  }
    }



    #[cfg(not(target_arch="wasm32"))]
    pub fn show_window<T: ItemFactory + 'static>(self) {

        use swsurface::{Format, SwWindow};

        use winit::{
            window::WindowBuilder,
        };

        let item = T::create();


        let event_loop = self.event_loop;
        let window = WindowBuilder::new().build(&event_loop).unwrap();


        let event_loop_proxy = event_loop.create_proxy();
        let sw_context = swsurface::ContextBuilder::new(&event_loop)
            .with_ready_cb(move |_| {
                let _ = event_loop_proxy.send_event(UserEvent::ReadyCB);
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

        let mut state = ApplicationState { cursor_pos : piet_common::kurbo::Point::ORIGIN };

        event_loop.run(move |event, _, control_flow| {
            let repaint = match event {
                Event::WindowEvent { event: WindowEvent::Resized(_), .. } |
                Event::WindowEvent { event: WindowEvent::HiDpiFactorChanged(_), ..} => {
                    sw_window.update_surface_to_fit(format);
                    let [width, height] = sw_window.image_info().extent;
                    item.geometry().width.set(width.into());
                    item.geometry().height.set(height.into());
                    true
                }
                _ => {
                    process_event::<T>(&mut state, &*item, sw_window.window(), event, control_flow)
                }
            };
            if repaint {
                redraw(&sw_window, item.clone());
            }
        });

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
    }

    #[cfg(target_arch="wasm32")]
    pub fn show_window<T: ItemFactory + 'static>(self) {


        use winit::{
            window::{WindowBuilder},
            platform::web::WindowExtStdweb,
        };
        use stdweb::{traits::*, web::document};

        let item = T::create();

        let event_loop = self.event_loop;
        let window = WindowBuilder::new().build(&event_loop).unwrap();
        document().body().unwrap().append_child(&window.canvas());

        //window.set_fullscreen(Some(winit::window::Fullscreen::Borderless))
        let winit::dpi::LogicalSize{width, height} = window.inner_size();
        item.geometry().width.set(width.into());
        item.geometry().height.set(height.into());

        let mut state = ApplicationState { cursor_pos : piet_common::kurbo::Point::ORIGIN };

        event_loop.run(move |event, _, control_flow| {
            let repaint = match event {
                Event::WindowEvent { event: WindowEvent::Resized(_), .. } |
                Event::WindowEvent { event: WindowEvent::HiDpiFactorChanged(_), ..} => {
                    let winit::dpi::LogicalSize{width, height} = window.inner_size();
                    item.geometry().width.set(width.into());
                    item.geometry().height.set(height.into());
                    true
                }
                _ => {
                    process_event::<T>(&mut state, &*item, &window, event, control_flow)
                }
            };
            if repaint {
                redraw(&window, item.clone());
            }
        });


        fn redraw(window: &Window, item: Rc<dyn Item>)  {
            use stdweb::web::CanvasRenderingContext2d;
            use piet_cargoweb::WebRenderContext;

            let canvas = window.canvas();
            let mut can_ctx = canvas.get_context::<CanvasRenderingContext2d>().unwrap(); // FIXME unwrap;
            can_ctx.clear_rect(0.,0.,item.geometry().width(), item.geometry().height());
            let mut ctx = WebRenderContext::new(&mut can_ctx);
            if let Err(r) = item.draw(&mut ctx) {
                stdweb::console!(log, format!("Error drawing {:?}", r));
            }
        }
    }

}
