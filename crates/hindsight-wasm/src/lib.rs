use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::WebSocket;

/// Initialize panic hook for better error messages in browser console
#[wasm_bindgen(start)]
pub fn init() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();

    web_sys::console::log_1(&"🔍 Hindsight WASM client initialized".into());
}

/// Hindsight client that connects to the server via WebSocket
#[wasm_bindgen]
pub struct HindsightClient {
    url: String,
    ws: Option<WebSocket>,
}

#[wasm_bindgen]
impl HindsightClient {
    /// Create a new client (not yet connected)
    #[wasm_bindgen(constructor)]
    pub fn new(url: String) -> Self {
        web_sys::console::log_1(&format!("Creating HindsightClient for {}", url).into());

        Self {
            url,
            ws: None,
        }
    }

    /// Connect to the Hindsight server
    pub async fn connect(&mut self) -> Result<(), JsValue> {
        web_sys::console::log_1(&format!("Connecting to {}...", self.url).into());

        // For now, we'll use a simple WebSocket connection
        // TODO: Integrate with Rapace WebSocket transport
        let ws = WebSocket::new(&self.url)
            .map_err(|e| JsValue::from_str(&format!("Failed to create WebSocket: {:?}", e)))?;

        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        // Wait for connection to open
        let ws_clone = ws.clone();
        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            let onopen = Closure::wrap(Box::new(move |_| {
                web_sys::console::log_1(&"WebSocket connected!".into());
                resolve.call0(&JsValue::NULL).ok();
            }) as Box<dyn FnMut(JsValue)>);

            let onerror = Closure::wrap(Box::new(move |e: web_sys::ErrorEvent| {
                web_sys::console::error_1(&format!("WebSocket error: {:?}", e).into());
                reject.call1(&JsValue::NULL, &e.into()).ok();
            }) as Box<dyn FnMut(web_sys::ErrorEvent)>);

            ws_clone.set_onopen(Some(onopen.as_ref().unchecked_ref()));
            ws_clone.set_onerror(Some(onerror.as_ref().unchecked_ref()));

            onopen.forget();
            onerror.forget();
        });

        JsFuture::from(promise).await?;

        self.ws = Some(ws);

        Ok(())
    }

    /// List recent traces
    pub async fn list_traces(&self) -> Result<JsValue, JsValue> {
        web_sys::console::log_1(&"Listing traces...".into());

        // For now, return mock data
        // TODO: Implement actual Rapace RPC call to list_traces()
        let mock_traces = js_sys::Array::new();

        for i in 0..5 {
            let trace = js_sys::Object::new();
            js_sys::Reflect::set(&trace, &"trace_id".into(), &format!("trace-{}", i).into())?;
            js_sys::Reflect::set(&trace, &"name".into(), &format!("test_span_{}", i).into())?;
            js_sys::Reflect::set(&trace, &"span_count".into(), &(i + 3).into())?;
            js_sys::Reflect::set(&trace, &"duration_ms".into(), &(100 + i * 50).into())?;
            mock_traces.push(&trace);
        }

        Ok(mock_traces.into())
    }

    /// Get a specific trace by ID
    pub async fn get_trace(&self, trace_id: String) -> Result<JsValue, JsValue> {
        web_sys::console::log_1(&format!("Getting trace {}...", trace_id).into());

        // For now, return mock data
        // TODO: Implement actual Rapace RPC call to get_trace()
        let trace = js_sys::Object::new();
        js_sys::Reflect::set(&trace, &"trace_id".into(), &trace_id.into())?;
        js_sys::Reflect::set(&trace, &"root_span_name".into(), &"test_span".into())?;

        let spans = js_sys::Array::new();
        for i in 0..3 {
            let span = js_sys::Object::new();
            js_sys::Reflect::set(&span, &"span_id".into(), &format!("span-{}", i).into())?;
            js_sys::Reflect::set(&span, &"name".into(), &format!("operation_{}", i).into())?;
            js_sys::Reflect::set(&span, &"start_time".into(), &(i * 100).into())?;
            js_sys::Reflect::set(&span, &"end_time".into(), &((i + 1) * 100).into())?;
            spans.push(&span);
        }
        js_sys::Reflect::set(&trace, &"spans".into(), &spans)?;

        Ok(trace.into())
    }

    /// Close the connection
    pub fn close(&mut self) {
        if let Some(ws) = &self.ws {
            let _ = ws.close();
            web_sys::console::log_1(&"WebSocket closed".into());
        }
        self.ws = None;
    }
}
