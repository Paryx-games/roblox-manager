use std::{io, mem::size_of, ptr};
use windows_sys::Win32::{
    Foundation::GlobalFree,
    System::{
        DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
        Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
        Ole::CF_UNICODETEXT,
    },
};

pub fn copy_text(text: &str) -> io::Result<()> {
    let text = text
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let size = text.len() * size_of::<u16>();
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, size) };
    if memory.is_null() {
        return Err(io::Error::other("Clipboard memory allocation failed"));
    }

    let destination = unsafe { GlobalLock(memory) }.cast::<u16>();
    if destination.is_null() {
        unsafe {
            GlobalFree(memory);
        }
        return Err(io::Error::other("Clipboard memory could not be locked"));
    }
    unsafe {
        ptr::copy_nonoverlapping(text.as_ptr(), destination, text.len());
        GlobalUnlock(memory);
    }

    if unsafe { OpenClipboard(ptr::null_mut()) } == 0 {
        unsafe {
            GlobalFree(memory);
        }
        return Err(io::Error::other("Windows clipboard is unavailable"));
    }

    let result = unsafe {
        let result = if EmptyClipboard() == 0 {
            Err(io::Error::other("Windows clipboard could not be cleared"))
        } else if SetClipboardData(CF_UNICODETEXT.into(), memory).is_null() {
            Err(io::Error::other("Windows clipboard could not be updated"))
        } else {
            Ok(())
        };
        CloseClipboard();
        result
    };

    if result.is_err() {
        unsafe {
            GlobalFree(memory);
        }
    }
    result
}
