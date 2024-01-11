use std::borrow::Cow;
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::Window,
};

async fn run(event_loop: EventLoop<()>, window: Window) {
    let mut size = window.inner_size();
    size.width = size.width.max(1);
    size.height = size.height.max(1);

    let instance = wgpu::Instance::default();

    let surface = instance.create_surface(&window).unwrap();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            // Request an adapter which can render to our surface
            compatible_surface: Some(&surface),
        })
        .await
        .expect("Failed to find an appropriate adapter");

    // Create the logical device and command queue
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty() | wgpu::Features::MULTI_DRAW_INDIRECT,
                // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the swapchain.
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
            },
            None,
        )
        .await
        .expect("Failed to create device");

    // Load the shaders from disk
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let swapchain_capabilities = surface.get_capabilities(&adapter);
    let swapchain_format = swapchain_capabilities.formats[0];

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(swapchain_format.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    #[derive(bytemuck::Pod, bytemuck::Zeroable, Copy, Clone)]
    #[repr(C)]
    pub struct DrawIndirect {
        pub vertex_count: u32,
        pub instance_count: u32,
        pub vertex_offset: i32,
        pub base_instance: u32,
    }
    #[derive(bytemuck::Pod, bytemuck::Zeroable, Copy, Clone)]
    #[repr(C)]
    pub struct DrawIndexedIndirect {
        pub vertex_count: u32,
        pub instance_count: u32,
        pub base_index: u32,
        pub vertex_offset: i32,
        pub base_instance: u32,
    }

    let target_count = std::env::var("INDIRECT_COUNT").unwrap().parse::<usize>().expect("missing INDIRECT_COUNT environment var; 419430 should work, 419431 should crash");
    let indexed = std::env::var("INDIRECT_INDEXED").unwrap().parse::<bool>().expect("missing INDIRECT_INDEXED environment var");

    let indirect_count;
    let bytes;
    let data_a;
    let data_b;

    if !indexed {
        let draw = DrawIndirect {
            vertex_count: 3,
            instance_count: 0,
            vertex_offset: 0,
            base_instance: 0,
        };
        let mut data = vec![draw; target_count - 1];
        data.push(DrawIndirect { vertex_count: 3, instance_count: 1, vertex_offset: 0, base_instance: 0 });

        indirect_count = data.len() as u32;
        data_a = data;
        bytes = bytemuck::cast_slice(&data_a);
    } else {
        let draw = DrawIndexedIndirect {
            vertex_count: 3,
            instance_count: 0,
            base_index: 0,
            vertex_offset: 0,
            base_instance: 0,
        };
        let mut data = vec![draw; target_count - 1];
        data.push(DrawIndexedIndirect { vertex_count: 3, instance_count: 1, base_index: 0, vertex_offset: 0, base_instance: 0 });

        indirect_count = data.len() as u32;
        data_b = data;
        bytes = bytemuck::cast_slice(&data_b);
    }

    let indirect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("indirect buffer"),
        size: 16 * 1024 * 1024,
        usage: wgpu::BufferUsages::INDIRECT,
        mapped_at_creation: true,
    });
    indirect_buffer.slice(..).get_mapped_range_mut()[..bytes.len()]
        .copy_from_slice(bytes);
    indirect_buffer.unmap();


    use wgpu::util::DeviceExt;
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("indirect buffer"),
        usage: wgpu::BufferUsages::INDEX,
        contents: bytemuck::cast_slice(&[0u16, 1, 2]),
    });
    let index_buffer_format = wgpu::IndexFormat::Uint16;


    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: swapchain_format,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: swapchain_capabilities.alpha_modes[0],
        view_formats: vec![],
    };

    surface.configure(&device, &config);

    let window = &window;
    event_loop
        .run(move |event, target| {
            // Have the closure take ownership of the resources.
            // `event_loop.run` never returns, therefore we must do this to ensure
            // the resources are properly cleaned up.
            let _ = (&instance, &adapter, &shader, &pipeline_layout);

            if let Event::WindowEvent {
                window_id: _,
                event,
            } = event
            {
                match event {
                    WindowEvent::Resized(new_size) => {
                        // Reconfigure the surface with the new size
                        config.width = new_size.width.max(1);
                        config.height = new_size.height.max(1);
                        surface.configure(&device, &config);
                        // On macos the window needs to be redrawn manually after resizing
                        window.request_redraw();
                    }
                    WindowEvent::RedrawRequested => {
                        let frame = surface
                            .get_current_texture()
                            .expect("Failed to acquire next swap chain texture");
                        let view = frame
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default());
                        let mut encoder =
                            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: None,
                            });
                        {
                            let mut rpass =
                                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                    label: None,
                                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                        view: &view,
                                        resolve_target: None,
                                        ops: wgpu::Operations {
                                            load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                                            store: wgpu::StoreOp::Store,
                                        },
                                    })],
                                    depth_stencil_attachment: None,
                                    timestamp_writes: None,
                                    occlusion_query_set: None,
                                });
                            rpass.set_pipeline(&render_pipeline);

                            // rpass.draw(0..3, 0..1);
                            if !indexed {
                                rpass.multi_draw_indirect(&indirect_buffer, 0, indirect_count);
                            } else {
                                rpass.set_index_buffer(index_buffer.slice(..), index_buffer_format);
                                rpass.multi_draw_indexed_indirect(&indirect_buffer, 0, indirect_count);
                            }
                        }

                        queue.submit(Some(encoder.finish()));
                        frame.present();
                    }
                    WindowEvent::CloseRequested => target.exit(),
                    _ => {}
                };
            }
        })
        .unwrap();
}

pub fn main() {
    let event_loop = EventLoop::new().unwrap();
    #[allow(unused_mut)]
    let mut builder = winit::window::WindowBuilder::new();
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use winit::platform::web::WindowBuilderExtWebSys;
        let canvas = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .get_element_by_id("canvas")
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();
        builder = builder.with_canvas(Some(canvas));
    }
    let window = builder.build(&event_loop).unwrap();

    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
        pollster::block_on(run(event_loop, window));
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        wasm_bindgen_futures::spawn_local(run(event_loop, window));
    }
}
