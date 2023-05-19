use std::sync::Once;

use windows::{
    core::HSTRING,
    w,
    Win32::{
        Foundation::{HWND, WPARAM, LPARAM, LRESULT},
        Graphics::Direct2D::ID2D1Factory1,
        System::Com::{CoInitializeEx, COINIT_MULTITHREADED},
        UI::WindowsAndMessaging::{
            CreateWindowExW, RegisterClassW, ShowWindow, CW_USEDEFAULT, SW_SHOW, WNDCLASSW,
            WS_OVERLAPPEDWINDOW, DefWindowProcW, WM_DESTROY, PostQuitMessage,
        },
    },
};

static REGISTER_WINDOW_CLASS: Once = Once::new();

fn main() -> windows::core::Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)?;
    }
    Ok(())
}

pub struct AppWindow<'a> {
    hwnd: HWND,
    factory: &'a ID2D1Factory1,
}

impl<'a> AppWindow<'a> {
    pub fn new(
        title: &'static str,
        factory: &'a ID2D1Factory1,
    ) -> windows::core::Result<Box<Self>> {
        let window_class = w!("mars.window.voronoi");
        REGISTER_WINDOW_CLASS.call_once(|| {
            let class_name = HSTRING::from("AppWindow");
            let class_name = class_name.as_wide();
            let class_name = class_name.as_ptr();
            let class = WNDCLASSW {
                lpfnWndProc: Some(Self::window_proc),
                lpszClassName: window_class,
                ..Default::default()
            };
            assert_ne!(unsafe { RegisterClassW(&class) }, 0);
        });
        let mut app_window = Box::new(Self {
            hwnd: HWND(0),
            factory,
        });
        let hwnd = unsafe {
            CreateWindowExW(
                Default::default(),
                window_class,
                &HSTRING::from(title),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                None,
                Some(app_window.as_mut() as *mut _ as _),
            )
        };
        unsafe { ShowWindow(hwnd, SW_SHOW) };
        Ok(app_window)
    }

    fn message_loop(&mut self, hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> windows::Win32::Foundation::LRESULT {
        match message {
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match message {
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
