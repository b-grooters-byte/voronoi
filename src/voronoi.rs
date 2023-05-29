use std::{sync::Once, cmp};

use rand::Rng;
use windows::{
    core::{Result, HSTRING},
    w,
    Win32::{
        Foundation::{COLORREF, HMODULE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::{
            Direct2D::{
                Common::{D2D1_COLOR_F, D2D_POINT_2F},
                ID2D1Factory1, ID2D1HwndRenderTarget, ID2D1SolidColorBrush, ID2D1StrokeStyle,
                D2D1_ELLIPSE, D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS,
                D2D1_RENDER_TARGET_PROPERTIES,
            },
            Gdi::{BeginPaint, CreateSolidBrush, EndPaint, InvalidateRect, PAINTSTRUCT},
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, GetClientRect, GetWindowLongPtrA, GetWindowRect,
            LoadCursorW, SetWindowLongPtrA, CREATESTRUCTA, CW_USEDEFAULT, GWLP_USERDATA, HMENU,
            IDC_ARROW, WINDOW_EX_STYLE, WM_CREATE, WM_MOUSEMOVE, WM_PAINT, WM_SIZE, WS_CHILDWINDOW,
            WS_CLIPSIBLINGS, WS_VISIBLE,
        },
    },
};

use crate::direct2d::{create_brush, create_stroke_style};

const PARABOLA_X_STEP: usize = 5;

static REGISTER_VORONOI_WINDOW_CLASS: Once = Once::new();

pub struct Voronoi<'a> {
    hwnd: HWND,
    factory: &'a ID2D1Factory1,
    target: Option<ID2D1HwndRenderTarget>,
    site_count: u16,
    sites: Vec<Site>,
    site_brush: Option<ID2D1SolidColorBrush>,
    sweep_line: f32,
    sweep_line_brush: Option<ID2D1SolidColorBrush>,
    beach_line_brush: Option<ID2D1SolidColorBrush>,
    default_line_style: ID2D1StrokeStyle,

}

impl<'a> Voronoi<'a> {
    pub fn new(sites: u16, parent: HWND, factory: &'a ID2D1Factory1) -> Result<Box<Self>> {
        let instance = unsafe { GetModuleHandleW(None)? };
        let line_style = create_stroke_style(factory, None)?;
        let class_name = w!("mars.window.voronoi.view");

        REGISTER_VORONOI_WINDOW_CLASS.call_once(|| {
            let class = windows::Win32::UI::WindowsAndMessaging::WNDCLASSW {
                lpfnWndProc: Some(Self::window_proc),
                hInstance: instance,
                hCursor: unsafe { LoadCursorW(HMODULE(0), IDC_ARROW).ok().unwrap() },
                hbrBackground: unsafe { CreateSolidBrush(COLORREF(0)) },
                lpszClassName: class_name,
                ..Default::default()
            };
            unsafe { windows::Win32::UI::WindowsAndMessaging::RegisterClassW(&class) };
        });

        let mut voronoi = Box::new(Self {
            hwnd: HWND(0),
            factory,
            target: None,
            site_count: sites,
            sites: Vec::new(),
            site_brush: None,
            sweep_line: 0.0,
            sweep_line_brush: None,
            beach_line_brush: None,
            default_line_style: line_style,
        });

        voronoi.random_sites(100, 100);

        let _window = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                &HSTRING::from(""),
                WS_VISIBLE | WS_CLIPSIBLINGS | WS_CHILDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT, //width as i32,
                CW_USEDEFAULT, // height as i32,
                parent,
                HMENU(0),
                instance,
                Some(voronoi.as_mut() as *mut _ as _),
            )
        };
        Ok(voronoi)
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn render(&mut self) -> Result<()> {
        if self.target.is_none() {
            self.create_render_target()?;
            self.create_device_resources()?;
        }
        let mut rect: RECT = RECT::default();
        unsafe { GetClientRect(self.hwnd, &mut rect) };
        let target = self.target.as_ref().unwrap();
        let site_brush = self.site_brush.as_ref().unwrap();
        let sweep_line_brush = self.sweep_line_brush.as_ref().unwrap();
        let line_style = &self.default_line_style;
        unsafe {
            target.BeginDraw();
            target.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }));
        }
        for site in &self.sites {
            unsafe {
                target.DrawEllipse(
                    &D2D1_ELLIPSE {
                        point: D2D_POINT_2F {
                            x: site.x,
                            y: site.y,
                        },
                        radiusX: 2.0,
                        radiusY: 2.0,
                    },
                    site_brush,
                    1.0,
                    line_style,
                );
            }
        }
        unsafe {
            target.DrawLine(
                D2D_POINT_2F {
                    x: 0.0,
                    y: self.sweep_line,
                },
                D2D_POINT_2F {
                    x: rect.right as f32,
                    y: self.sweep_line,
                },
                sweep_line_brush,
                1.0,
                line_style,
            )
        };
        self.render_beach_line(&rect, target)?;
        unsafe { target.EndDraw(None, None)? };
        Ok(())
    }

    fn render_beach_line(&self, clip: &RECT, target: &ID2D1HwndRenderTarget) -> Result<()> {
        let line_style = &self.default_line_style;
        let beach_line_brush = self.beach_line_brush.as_ref().unwrap();
        for site in &self.sites {
            if site.y <= self.sweep_line {
                let mut rendering = false;
                let mut stop_render = false;
                let mut prev = D2D_POINT_2F { x: 0.0, y: 0.0 }; 
                let start_x = cmp::max(clip.left - PARABOLA_X_STEP as i32, 0) as usize;
                let end_x = clip.right as usize + PARABOLA_X_STEP;
                for x in (start_x..=end_x).step_by(PARABOLA_X_STEP) {
                    let y = 1.0 / (2.0 * (site.y - self.sweep_line)) * ((x as f32 - site.x) * (x as f32 - site.x))
                        + ((site.y + self.sweep_line) / 2.0);
                    if rendering {
                        let p = D2D_POINT_2F { x: x as f32, y };
                        unsafe {target.DrawLine(prev, p, beach_line_brush, 1.0, line_style) };
                    }else if y > 0.0 && y < self.sweep_line && !rendering {
                        rendering = true;
                    } 

                    prev.x = x as f32;
                    prev.y = y;
                    if stop_render {
                        break;
                    }
                    stop_render = rendering && (y < 0.0 );
                }            
            }
        }
        Ok(())
    }

    fn random_sites(&mut self, width: u32, height: u32) {
        self.sites.clear();
        let mut rng = rand::thread_rng();
        for _ in 0..self.site_count {
            let x = rng.gen_range(0.0..=width as f32);
            let y = rng.gen_range(0.0..=height as f32);
            self.sites.push(Site { x, y });
        }
        self.sites.sort_by(|&a, &b| a.partial_cmp(&b).unwrap());
    }

    fn create_render_target(&mut self) -> Result<()> {
        let mut rect: RECT = RECT::default();
        unsafe { GetClientRect(self.hwnd, &mut rect) };
        let props = D2D1_RENDER_TARGET_PROPERTIES::default();
        let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd: self.hwnd,
            pixelSize: windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_U {
                width: (rect.right - rect.left) as u32,
                height: (rect.bottom - rect.top) as u32,
            },
            presentOptions: D2D1_PRESENT_OPTIONS::default(),
        };
        let target = unsafe { self.factory.CreateHwndRenderTarget(&props, &hwnd_props)? };
        self.target = Some(target);

        Ok(())
    }

    fn create_device_resources(&mut self) -> Result<()> {
        self.site_brush = Some(create_brush(
            self.target.as_ref().unwrap(),
            1.0,
            0.0,
            0.0,
            1.0,
        )?);
        self.sweep_line_brush = Some(create_brush(
            self.target.as_ref().unwrap(),
            0.0,
            1.0,
            0.0,
            1.0,
        )?);
        self.beach_line_brush = Some(create_brush(
            self.target.as_ref().unwrap(),
            0.0,
            0.0,
            1.0,
            1.0,
        )?);
        Ok(())
    }

    fn release_device_resources(&mut self) {
        self.site_brush = None;
        self.sweep_line_brush = None;
        self.beach_line_brush = None;
    }

    fn message_handler(&mut self, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match message {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                unsafe {
                    BeginPaint(self.hwnd, &mut ps);
                    self.render().expect("unable to render");
                    EndPaint(self.hwnd, &ps);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let prev_sweep_line = self.sweep_line;
                let pos = mouse_position(lparam);
                self.sweep_line = pos.1;
                let mut rect: RECT = RECT::default();
                unsafe { GetWindowRect(self.hwnd, &mut rect) };
                unsafe {
                    if prev_sweep_line <= self.sweep_line {
                        rect.top = prev_sweep_line as i32;
                        rect.bottom = self.sweep_line as i32;
                    } else {
                        rect.top = self.sweep_line as i32;
                        rect.bottom = prev_sweep_line as i32;
                    }
                    InvalidateRect(self.hwnd, Some(&rect), true);
                }
                LRESULT(0)
            }
            WM_SIZE => {
                let size = size(lparam);
                self.random_sites(size.0, size.1);
                self.release_device_resources();
                self.create_render_target()
                    .expect("unable to create render target");
                self.create_device_resources()
                    .expect("unable to create device resources");
                unsafe {
                    InvalidateRect(self.hwnd, None, true);
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(self.hwnd, message, wparam, lparam) },
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_CREATE {
            let create_struct = lparam.0 as *const CREATESTRUCTA;
            let this = (*create_struct).lpCreateParams as *mut Self;
            (*this).hwnd = hwnd;

            SetWindowLongPtrA(hwnd, GWLP_USERDATA, this as _);
        } else {
            let this = GetWindowLongPtrA(hwnd, GWLP_USERDATA) as *mut Self;

            if !this.is_null() {
                return (*this).message_handler(message, wparam, lparam);
            }
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
struct Site {
    x: f32,
    y: f32,
}

impl PartialOrd for Site {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.y.partial_cmp(&other.y)
    }
}

fn mouse_position(lparam: LPARAM) -> (f32, f32) {
    (
        (lparam.0 & 0x0000_FFFF) as f32,
        ((lparam.0 & 0xFFFF_0000) >> 16) as f32,
    )
}

fn size(lparam: LPARAM) -> (u32, u32) {
    (
        (lparam.0 & 0x0000_FFFF) as u32,
        ((lparam.0 & 0xFFFF_0000) >> 16) as u32,
    )
}
