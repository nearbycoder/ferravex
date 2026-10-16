//! First-party typed GL ABI. Every indirect procedure is resolved for the current
//! OS context before rendering; desktop Linux uses its linked system GL symbols.
#![allow(non_snake_case, dead_code)]
use std::ffi::{c_char, c_int, c_uint, c_void};
macro_rules! functions {
 ($($name:ident($($arg:ident:$ty:ty),*) $(->$ret:ty)?;)*)=>{
  #[cfg(target_os="linux")]
  #[link(name="GL")]
  unsafe extern "C" {$(pub(super) fn $name($($arg:$ty),*) $(->$ret)?;)*}
  #[cfg(not(target_os="linux"))]
  struct Api { $($name: Option<unsafe extern "system" fn($($ty),*) $(->$ret)?>,)* }
  #[cfg(not(target_os="linux"))]
  thread_local! {static API:std::cell::RefCell<Option<Api>>=const {std::cell::RefCell::new(None)};}
  #[cfg(not(target_os="linux"))]
  pub(super) fn load()->Result<(),String>{
   $(let $name=unsafe {
     let name=std::ffi::CString::new(stringify!($name)).unwrap();
     let address=super::host::proc_address(&name);
     if address.is_null() {
      if !optional(stringify!($name)) {return Err(format!("Missing GPU procedure {}",stringify!($name)));}
      None
     } else {Some(std::mem::transmute::<*const c_void,unsafe extern "system" fn($($ty),*) $(->$ret)?>(address))}
   };)*
   API.with(|api|*api.borrow_mut()=Some(Api {$($name,)*})); Ok(())
  }
  #[cfg(not(target_os="linux"))]
  pub(super) fn timer_available()->bool {API.with(|api|api.borrow().as_ref().is_some_and(|a|a.glGenQueries.is_some()&&a.glDeleteQueries.is_some()&&a.glBeginQuery.is_some()&&a.glEndQuery.is_some()&&a.glGetQueryObjectiv.is_some()&&a.glGetQueryObjectui64v.is_some()))}
  $(#[cfg(not(target_os="linux"))]
  #[allow(clippy::too_many_arguments)] // Fixed system GL ABI.
  pub(super) unsafe fn $name($($arg:$ty),*) $(->$ret)? {
   API.with(|api| {let api=api.borrow();let function=api.as_ref().and_then(|api|api.$name).expect("current context GL procedure");unsafe {function($($arg),*)}})
  })*
 }
}
#[cfg(not(target_os = "linux"))]
fn optional(name: &str) -> bool {
    matches!(
        name,
        "glGenQueries"
            | "glDeleteQueries"
            | "glBeginQuery"
            | "glEndQuery"
            | "glGetQueryObjectiv"
            | "glGetQueryObjectui64v"
    )
}
functions! {
    glViewport(x: c_int, y: c_int, width: c_int, height: c_int);
    glClearColor(r: f32, g: f32, b: f32, a: f32);
    glClearDepth(depth: f64);
    glClear(mask: c_uint);
    glEnable(capability: c_uint);
    glDisable(capability: c_uint);
    glDepthFunc(function: c_uint);
    glDepthMask(enabled: u8);
    glBlendFuncSeparate(
        source_rgb: c_uint,
        destination_rgb: c_uint,
        source_alpha: c_uint,
        destination_alpha: c_uint);
    glCreateShader(kind: c_uint) -> c_uint;
    glShaderSource(
        shader: c_uint,
        count: c_int,
        source: *const *const c_char,
        length: *const c_int);
    glCompileShader(shader: c_uint);
    glGetShaderiv(shader: c_uint, parameter: c_uint, value: *mut c_int);
    glGetShaderInfoLog(
        shader: c_uint,
        size: c_int,
        length: *mut c_int,
        log: *mut c_char);
    glDeleteShader(shader: c_uint);
    glCreateProgram() -> c_uint;
    glAttachShader(program: c_uint, shader: c_uint);
    glBindAttribLocation(program: c_uint, index: c_uint, name: *const c_char);
    glLinkProgram(program: c_uint);
    glGetProgramiv(program: c_uint, parameter: c_uint, value: *mut c_int);
    glGetProgramInfoLog(
        program: c_uint,
        size: c_int,
        length: *mut c_int,
        log: *mut c_char);
    glDeleteProgram(program: c_uint);
    glUseProgram(program: c_uint);
    glGenBuffers(count: c_int, buffers: *mut c_uint);
    glDeleteBuffers(count: c_int, buffers: *const c_uint);
    glBindBuffer(target: c_uint, buffer: c_uint);
    glBufferData(target: c_uint, bytes: isize, data: *const c_void, usage: c_uint);
    glBufferSubData(target: c_uint, offset: isize, bytes: isize, data: *const c_void);
    glEnableVertexAttribArray(index: c_uint);
    glDisableVertexAttribArray(index: c_uint);
    glVertexAttribPointer(
        index: c_uint,
        components: c_int,
        kind: c_uint,
        normalized: u8,
        stride: c_int,
        offset: *const c_void);
    glDrawArrays(mode: c_uint, first: c_int, count: c_int);
    glDrawArraysInstanced(mode: c_uint, first: c_int, count: c_int, instances: c_int);
    glVertexAttribDivisor(index: c_uint, divisor: c_uint);
    glGetUniformLocation(program: c_uint, name: *const c_char) -> c_int;
    glUniformMatrix4fv(
        location: c_int,
        count: c_int,
        transpose: u8,
        values: *const f32);
    glUniformMatrix3fv(
        location: c_int,
        count: c_int,
        transpose: u8,
        values: *const f32);
    glGenQueries(count: c_int, queries: *mut c_uint);
    glDeleteQueries(count: c_int, queries: *const c_uint);
    glBeginQuery(target: c_uint, query: c_uint);
    glEndQuery(target: c_uint);
    glGetQueryObjectiv(query: c_uint, parameter: c_uint, value: *mut c_int);
    glGetQueryObjectui64v(query: c_uint, parameter: c_uint, value: *mut u64);
    glUniform1i(location: c_int, value: c_int);
    glGenTextures(count: c_int, textures: *mut c_uint);
    glDeleteTextures(count: c_int, textures: *const c_uint);
    glBindTexture(target: c_uint, texture: c_uint);
    glActiveTexture(texture: c_uint);
    glTexParameteri(target: c_uint, parameter: c_uint, value: c_int);
    glTexImage2D(
        target: c_uint,
        level: c_int,
        internal: c_int,
        width: c_int,
        height: c_int,
        border: c_int,
        format: c_uint,
        kind: c_uint,
        data: *const c_void);
    glGenFramebuffers(count: c_int, framebuffers: *mut c_uint);
    glDeleteFramebuffers(count: c_int, framebuffers: *const c_uint);
    glBindFramebuffer(target: c_uint, framebuffer: c_uint);
    glFramebufferTexture2D(
        target: c_uint,
        attachment: c_uint,
        texture_target: c_uint,
        texture: c_uint,
        level: c_int);
    glCheckFramebufferStatus(target: c_uint) -> c_uint;
    glDrawBuffer(buffer: c_uint);
    glUniform4fv(location: c_int, count: c_int, values: *const f32);
    glReadBuffer(buffer: c_uint);
    glPixelStorei(parameter: c_uint, value: c_int);
    glReadPixels(
        x: c_int,
        y: c_int,
        width: c_int,
        height: c_int,
        format: c_uint,
        kind: c_uint,
        pixels: *mut c_void);
    glFinish();
    glGetString(name: c_uint) -> *const u8;
    glGenRenderbuffers(n:c_int,buffers:*mut c_uint);
    glDeleteRenderbuffers(n:c_int,buffers:*const c_uint);
    glBindRenderbuffer(target:c_uint,buffer:c_uint);
    glRenderbufferStorage(target:c_uint,format:c_uint,width:c_int,height:c_int);
    glGetRenderbufferParameteriv(target:c_uint,name:c_uint,value:*mut c_int);
    glFramebufferRenderbuffer(target:c_uint,attachment:c_uint,renderbuffer_target:c_uint,renderbuffer:c_uint);
    glGenVertexArrays(n: c_int, arrays: *mut c_uint);
    glBindVertexArray(array: c_uint);
    glDeleteVertexArrays(n: c_int, arrays: *const c_uint);
}

#[cfg(target_os = "linux")]
pub(super) fn timer_available() -> bool {
    true
}
