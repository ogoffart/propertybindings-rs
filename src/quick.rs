use super::items::{Item, MouseEvent};
use std::rc::Rc;


/// Use as a factory for RSMLItem
pub trait ItemFactory {
    fn create() -> Rc<dyn Item<'static>>;
    fn tick() {}
}

#[cfg(not(target_arch="wasm32"))]
pub fn show_window<T: ItemFactory + 'static>() {

    use swsurface::{Format, SwWindow};

    use winit::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
        window::WindowBuilder,
    };

    let item = T::create();


    enum UserEvent { ReadyCB, Timer };

    let event_loop = EventLoop::<UserEvent>::with_user_event();
    let window = WindowBuilder::new().build(&event_loop).unwrap();


    let event_loop_proxy = event_loop.create_proxy();
    let sw_context = swsurface::ContextBuilder::new(&event_loop)
        .with_ready_cb(move |_| {
            let _ = event_loop_proxy.send_event(UserEvent::ReadyCB);
        })
        .build();

    let event_loop_proxy2 = event_loop.create_proxy();
    ::std::thread::spawn(move || {
        loop {
            ::std::thread::sleep(std::time::Duration::from_millis(16));
            let _ = event_loop_proxy2.send_event(UserEvent::Timer);
        }
    });


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
            Event::UserEvent(UserEvent::Timer) => {
                T::tick();
                // FIXME: listen on property changes
                sw_window.window().request_redraw();
            }
            _ => *control_flow = ControlFlow::Wait,
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
pub fn show_window<T: ItemFactory + 'static>() {


    use winit::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
        window::{WindowBuilder, Window},
        platform::web::WindowExtStdweb,
    };
    use stdweb::{traits::*, web::document};

    let item = T::create();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();
    document().body().unwrap().append_child(&window.canvas());
    let mut cursor_pos = piet_common::kurbo::Point::ORIGIN;

    //window.set_fullscreen(Some(winit::window::Fullscreen::Borderless))
    let winit::dpi::LogicalSize{width, height} = window.inner_size();
    item.geometry().width.set(width.into());
    item.geometry().height.set(height.into());




    event_loop.run(move |event, _, control_flow| {

       /* let messages = format!("{:#?} ", event);
        js! {
            document.write("<h5>" + @{messages} + "</h5>");
        };*/
//         stdweb::console!(log, format!("{:?}", event));

        match event {
            Event::WindowEvent { event: WindowEvent::Resized(_), .. } |
            Event::WindowEvent { event: WindowEvent::HiDpiFactorChanged(_), ..} => {
                let winit::dpi::LogicalSize{width, height} = window.inner_size();
                item.geometry().width.set(width.into());
                item.geometry().height.set(height.into());
                redraw(&window, item.clone());
            }

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
                redraw(&window, item.clone());
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
                let f = window.hidpi_factor();
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
                window.request_redraw();
            }
            Event::UserEvent(_) => {
                T::tick();
                // FIXME: listen on property changes
                window.request_redraw();
            }
            _ => {},
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
