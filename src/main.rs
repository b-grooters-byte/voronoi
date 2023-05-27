mod direct2d;
mod voronoi;

use std::sync::Once;

use windows::{
    core::{Result, HSTRING},
    w,
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Direct2D::ID2D1Factory1,
        System::Com::{CoInitializeEx, COINIT_MULTITHREADED},
        UI::WindowsAndMessaging::{
            AdjustWindowRect, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
            GetWindowLongPtrA, GetWindowRect, PostQuitMessage, RegisterClassW, SetWindowLongPtrA,
            SetWindowPos, ShowWindow, CREATESTRUCTA, CW_USEDEFAULT, GWLP_USERDATA, MSG, SWP_NOMOVE,
            SW_SHOW, WM_CREATE, WM_DESTROY, WM_MOUSEMOVE, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
            WS_VISIBLE,
        },
    },
};

static REGISTER_WINDOW_CLASS: Once = Once::new();

fn main() -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)?;
    }
    let factory = direct2d::create_factory()?;
    let _m = AppWindow::new("Voronoi", &factory);
    let mut message = MSG::default();
    unsafe {
        while GetMessageW(&mut message, HWND(0), 0, 0).into() {
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

pub struct AppWindow<'a> {
    hwnd: HWND,
    factory: &'a ID2D1Factory1,
    voronoi: Option<Box<voronoi::Voronoi<'a>>>,
}

impl<'a> AppWindow<'a> {
    pub fn new(
        title: &'static str,
        factory: &'a ID2D1Factory1,
    ) -> windows::core::Result<Box<Self>> {
        let window_class = w!("mars.window.voronoi");
        REGISTER_WINDOW_CLASS.call_once(|| {
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
            voronoi: None,
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

    fn message_loop(
        &mut self,
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        match message {
            WM_CREATE => match voronoi::Voronoi::new(100, self.hwnd, self.factory) {
                Ok(v) => {
                    self.voronoi = Some(v);
                    LRESULT(0)
                }
                Err(e) => LRESULT(-1),
            },
            WM_SIZE => {
                if self.voronoi.is_none() {
                    return LRESULT(0);
                }
                let mut rect = RECT::default();
                unsafe {
                    GetWindowRect(self.hwnd, &mut rect);
                    AdjustWindowRect(&mut rect, WS_VISIBLE | WS_OVERLAPPEDWINDOW, false);
                    SetWindowPos(
                        self.voronoi.as_ref().unwrap().hwnd(),
                        None,
                        rect.left,
                        rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                        SWP_NOMOVE,
                    );
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
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
        }
        let this = GetWindowLongPtrA(hwnd, GWLP_USERDATA) as *mut Self;

        if !this.is_null() {
            return (*this).message_loop(hwnd, message, wparam, lparam);
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}
