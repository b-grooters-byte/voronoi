use windows::{
    core::{HSTRING, Result},
    Win32::{Graphics::Direct2D::{Common::D2D1_COLOR_F, D2D1_RENDER_TARGET_PROPERTIES, ID2D1Factory1, ID2D1HwndRenderTarget, D2D1_PRESENT_OPTIONS, D2D1_HWND_RENDER_TARGET_PROPERTIES}, Foundation::{RECT, HWND}, UI::WindowsAndMessaging::GetClientRect}
};

pub struct Voronoi<'a> {
    hwnd: HWND,
    factory: &'a ID2D1Factory1,
    cell_count: u16,
    cells: Vec<Cell>,
    target: Option<ID2D1HwndRenderTarget>,
}


impl<'a> Voronoi<'a> {
    pub fn render(&mut self) -> Result<()> {
        self.create_render_target()?;
        unsafe { 
            self.target.as_ref().unwrap().BeginDraw();
            self.target.as_ref().unwrap().Clear(Some(&D2D1_COLOR_F{ r: 0.0, g: 0.0, b: 0.0, a:1.0}));
            self.target.as_ref().unwrap().EndDraw(None, None)?;
            };
        Ok(())
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
}


struct Cell {
    
}