//! Approved exception: only COM pointers, stream reads and GDI bitmap creation use unsafe.
//! Windows supplies valid ABI pointers. PDFs never supply addresses or allocation lengths here.
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use pdfcraft_render::{PageRenderer, RenderConfig};
use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_INVALIDARG, E_OUTOFMEMORY, E_POINTER, S_FALSE, S_OK};
use windows::Win32::Graphics::Gdi::{BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CreateDIBSection, DIB_RGB_COLORS, HBITMAP};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl, IStream};
use windows::Win32::UI::Shell::PropertiesSystem::{IInitializeWithStream, IInitializeWithStream_Impl};
use windows::Win32::UI::Shell::{IThumbnailProvider, IThumbnailProvider_Impl, WTS_ALPHATYPE, WTSAT_ARGB};
use windows::core::{BOOL, Error, GUID, HRESULT, IUnknown, Interface, Ref, Result, implement};

pub const CLSID: GUID = GUID::from_u128(0xd4b81246_583f_4868_9de4_d54b51f160a9);
const MAX_INPUT: usize = 64 * 1024 * 1024;
const MAX_THUMBNAIL: u32 = 1024;
static OBJECTS: AtomicU32 = AtomicU32::new(0);
static LOCKS: AtomicU32 = AtomicU32::new(0);

struct Lifetime;
impl Lifetime {
    fn new() -> Self {
        OBJECTS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}
impl Drop for Lifetime {
    fn drop(&mut self) {
        OBJECTS.fetch_sub(1, Ordering::SeqCst);
    }
}

fn guarded<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|_| Error::from_hresult(E_FAIL))?
}

#[implement(IInitializeWithStream, IThumbnailProvider)]
struct Provider {
    bytes: Mutex<Option<Arc<Vec<u8>>>>,
    _lifetime: Lifetime,
}

impl Provider {
    fn new() -> Self {
        Self { bytes: Mutex::new(None), _lifetime: Lifetime::new() }
    }
}

impl IInitializeWithStream_Impl for Provider_Impl {
    fn Initialize(&self, stream: Ref<'_, IStream>, mode: u32) -> Result<()> {
        guarded(|| {
            if mode & 3 != 0 {
                return Err(Error::from_hresult(E_INVALIDARG));
            }
            let stream = stream.as_ref().ok_or_else(|| Error::from_hresult(E_POINTER))?;
            let mut slot = self.bytes.lock().map_err(|_| Error::from_hresult(E_FAIL))?;
            if slot.is_some() {
                return Err(Error::from_hresult(HRESULT(0x800704df_u32 as i32))); // ERROR_ALREADY_INITIALIZED
            }
            *slot = Some(Arc::new(read_stream(stream)?));
            Ok(())
        })
    }
}

fn read_stream(stream: &IStream) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let mut read = 0;
        // SAFETY: chunk is writable for cb bytes; read lives until this synchronous call returns.
        unsafe { stream.Read(chunk.as_mut_ptr().cast(), chunk.len() as u32, Some(&mut read)) }.ok()?;
        let data = chunk.get(..read as usize).ok_or_else(|| Error::from_hresult(E_FAIL))?;
        if data.is_empty() {
            return Ok(bytes);
        }
        if bytes.len().saturating_add(data.len()) > MAX_INPUT {
            return Err(Error::new(E_FAIL, "PDF exceeds the 64 MiB thumbnail limit"));
        }
        bytes.try_reserve(data.len()).map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        bytes.extend_from_slice(data);
    }
}

impl IThumbnailProvider_Impl for Provider_Impl {
    fn GetThumbnail(&self, cx: u32, bitmap: *mut HBITMAP, alpha: *mut WTS_ALPHATYPE) -> Result<()> {
        if bitmap.is_null() || alpha.is_null() {
            return Err(Error::from_hresult(E_POINTER));
        }
        // SAFETY: COM gives writable out parameters; initialize before any fallible work.
        unsafe { *bitmap = HBITMAP::default(); *alpha = WTS_ALPHATYPE::default(); }
        guarded(|| {
            if cx == 0 {
                return Err(Error::from_hresult(E_INVALIDARG));
            }
            let bytes = self.bytes.lock().map_err(|_| Error::from_hresult(E_FAIL))?
                .clone().ok_or_else(|| Error::from_hresult(E_FAIL))?;
            let mut renderer = PageRenderer::new(bytes, RenderConfig::default());
            let page = renderer.thumbnail(0, cx.min(MAX_THUMBNAIL)).map_err(|e| Error::new(E_FAIL, e))?;
            let byte_count = (page.width as usize).checked_mul(page.height as usize)
                .and_then(|n| n.checked_mul(4)).ok_or_else(|| Error::from_hresult(E_FAIL))?;
            if page.rgba.len() != byte_count {
                return Err(Error::from_hresult(E_FAIL));
            }
            // Top-down 32-bit DIB: premultiplied BGRA, as required by IThumbnailProvider.
            let info = BITMAPINFO { bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: page.width as i32, biHeight: -(page.height as i32),
                biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0,
                ..Default::default()
            }, ..Default::default() };
            let mut bits = std::ptr::null_mut();
            // SAFETY: info is valid, dimensions are <=1024; bits is a writable out pointer.
            let handle = unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }?;
            if bits.is_null() {
                // SAFETY: this bitmap has not been handed to the shell and is ours to free.
                let _ = unsafe { windows::Win32::Graphics::Gdi::DeleteObject(handle.into()) };
                return Err(Error::from_hresult(E_FAIL));
            }
            // SAFETY: GDI allocated width*height*4 writable bytes, checked against the source above.
            let destination = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), byte_count) };
            for (source, target) in page.rgba.chunks_exact(4).zip(destination.chunks_exact_mut(4)) {
                target.copy_from_slice(source);
                target.swap(0, 2);
            }
            // SAFETY: ownership transfers to Windows, which deletes the bitmap after caching it.
            unsafe { *bitmap = handle; *alpha = WTSAT_ARGB; }
            Ok(())
        })
    }
}

#[implement(IClassFactory)]
struct Factory { _lifetime: Lifetime }

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Ref<'_, IUnknown>, iid: *const GUID, object: *mut *mut c_void) -> Result<()> {
        if object.is_null() || iid.is_null() { return Err(Error::from_hresult(E_POINTER)); }
        // SAFETY: caller provides a writable COM out parameter.
        unsafe { *object = std::ptr::null_mut(); }
        guarded(|| {
            if outer.as_ref().is_some() { return Err(Error::from_hresult(CLASS_E_NOAGGREGATION)); }
            let instance: IUnknown = Provider::new().into();
            // SAFETY: iid and object are supplied by COM; QueryInterface adds the returned reference.
            unsafe { instance.query(iid, object) }.ok()
        })
    }
    fn LockServer(&self, lock: BOOL) -> Result<()> {
        if lock.as_bool() { LOCKS.fetch_add(1, Ordering::SeqCst); }
        else {
            let mut count = LOCKS.load(Ordering::SeqCst);
            while count != 0 {
                match LOCKS.compare_exchange_weak(count, count - 1, Ordering::SeqCst, Ordering::SeqCst) {
                    Ok(_) => break,
                    Err(current) => count = current,
                }
            }
        }
        Ok(())
    }
}

/// SAFETY: this DLL entry point is called only with COM ABI pointers.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(clsid: *const GUID, iid: *const GUID, object: *mut *mut c_void) -> HRESULT {
    if clsid.is_null() || iid.is_null() || object.is_null() { return E_POINTER; }
    // SAFETY: COM supplies valid pointers after the null checks.
    unsafe { *object = std::ptr::null_mut(); }
    let result = guarded(|| {
        // SAFETY: clsid points to a COM class identifier.
        if unsafe { *clsid } != CLSID { return Err(Error::from_hresult(CLASS_E_CLASSNOTAVAILABLE)); }
        let factory: IUnknown = Factory { _lifetime: Lifetime::new() }.into();
        // SAFETY: QueryInterface owns validation of iid and adds the reference on success.
        unsafe { factory.query(iid, object) }.ok()
    });
    match result { Ok(()) => S_OK, Err(error) => error.code() }
}

#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if OBJECTS.load(Ordering::SeqCst) == 0 && LOCKS.load(Ordering::SeqCst) == 0 { S_OK } else { S_FALSE }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Gdi::{BITMAP, DeleteObject, GetObjectW};
    use windows::Win32::UI::Shell::SHCreateMemStream;

    fn pdf() -> Vec<u8> {
        use lopdf::{Document, Object, Stream, dictionary};
        let mut doc = Document::with_version("1.4");
        let pages = doc.new_object_id();
        let content = doc.add_object(Stream::new(dictionary! {}, b"1 0 0 rg 0 0 100 50 re f".to_vec()));
        let page = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages,
            "MediaBox" => vec![0.into(), 0.into(), 100.into(), 50.into()], "Contents" => content });
        doc.objects.insert(pages, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
        let root = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", root);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn com_factory_stream_and_bitmap_work_end_to_end() {
        assert_eq!(DllCanUnloadNow(), S_OK);
        let mut raw = std::ptr::null_mut();
        unsafe { DllGetClassObject(&CLSID, &IClassFactory::IID, &mut raw) }.ok().unwrap();
        let factory = unsafe { IClassFactory::from_raw(raw) };
        assert_eq!(DllCanUnloadNow(), S_FALSE);
        let provider: IThumbnailProvider = unsafe { factory.CreateInstance(None) }.unwrap();
        let initialize: IInitializeWithStream = provider.cast().unwrap();
        let bytes = pdf();
        let stream = unsafe { SHCreateMemStream(Some(&bytes)) }.unwrap();
        unsafe { initialize.Initialize(&stream, 0) }.unwrap();
        assert!(unsafe { initialize.Initialize(&stream, 0) }.is_err());
        let mut bitmap = HBITMAP::default();
        let mut alpha = WTS_ALPHATYPE::default();
        unsafe { provider.GetThumbnail(64, &mut bitmap, &mut alpha) }.unwrap();
        assert_eq!(alpha, WTSAT_ARGB);
        let mut info = BITMAP::default();
        assert_ne!(unsafe { GetObjectW(bitmap.into(), std::mem::size_of::<BITMAP>() as i32, Some((&mut info as *mut BITMAP).cast())) }, 0);
        assert_eq!((info.bmWidth, info.bmHeight, info.bmBitsPixel), (64, 32, 32));
        let pixels = unsafe { std::slice::from_raw_parts(info.bmBits.cast::<u8>(), 64 * 32 * 4) };
        assert!(pixels.chunks_exact(4).any(|p| p == [0, 0, 255, 255]), "red page must be BGRA, not RGBA");
        assert!(unsafe { DeleteObject(bitmap.into()) }.as_bool());
        assert!(unsafe { provider.GetThumbnail(0, &mut bitmap, &mut alpha) }.is_err());
        assert!(bitmap.is_invalid());
        let invalid: IThumbnailProvider = unsafe { factory.CreateInstance(None) }.unwrap();
        let bad_stream = unsafe { SHCreateMemStream(Some(b"not a PDF")) }.unwrap();
        unsafe { invalid.cast::<IInitializeWithStream>().unwrap().Initialize(&bad_stream, 0) }.unwrap();
        assert!(unsafe { invalid.GetThumbnail(64, &mut bitmap, &mut alpha) }.is_err());
        drop(invalid);
        drop(initialize);
        drop(provider);
        unsafe { factory.LockServer(true) }.unwrap();
        unsafe { factory.LockServer(false) }.unwrap();
        drop(factory);
        assert_eq!(DllCanUnloadNow(), S_OK);
    }
}
