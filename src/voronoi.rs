use std::sync::Once;

use rand::Rng;
use windows::{
    core::{Result, HSTRING},
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM, HMODULE},
        Graphics::{
            Direct2D::{
                Common::D2D1_COLOR_F, ID2D1Factory1, ID2D1HwndRenderTarget,
                D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS,
                D2D1_RENDER_TARGET_PROPERTIES,
            },
            Gdi::CreateSolidBrush,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            DefWindowProcW, GetClientRect, GetWindowLongPtrA, LoadCursorW, SetWindowLongPtrA,
            CREATESTRUCTA, GWLP_USERDATA, IDC_ARROW, WM_CREATE,
        },
    }, w,
};

use crate::direct2d::create_stroke_style;

static REGISTER_WINDOW_CLASS: Once = Once::new();

pub struct Voronoi<'a> {
    hwnd: HWND,
    factory: &'a ID2D1Factory1,
    target: Option<ID2D1HwndRenderTarget>,
    site_count: u16,
    sites: Vec<Site>,
    sweep_line: f32,
}

impl<'a> Voronoi<'a> {
    pub fn new(sites: u16, parent: HWND, factory: &'a ID2D1Factory1) -> Result<Box<Self>> {
        let instance = unsafe { GetModuleHandleW(None)? };
        let line_style = create_stroke_style(&factory, None)?;

        REGISTER_WINDOW_CLASS.call_once(|| {
            let class_name = w!("Voronoi");
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
            sweep_line: 0.0,
        });

        Ok(voronoi)
    }

    pub fn render(&mut self) -> Result<()> {
        self.create_render_target()?;
        unsafe {
            self.target.as_ref().unwrap().BeginDraw();
            self.target.as_ref().unwrap().Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }));
            self.target.as_ref().unwrap().EndDraw(None, None)?;
        };
        Ok(())
    }

    fn random_sites(&mut self) {
        let mut rng = rand::thread_rng();
        for _ in 0..self.site_count {
            let x = rng.gen_range(0.0..=1.0);
            let y = rng.gen_range(0.0..=1.0);
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

    fn message_handler(&mut self, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match message {
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
