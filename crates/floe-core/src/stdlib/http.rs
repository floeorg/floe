use super::*;

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let response = Type::Named("Response".to_string());
    let error = Type::Named("Error".to_string());

    fns.extend([
        stdlib_fn!("Http", "get", [Type::String], promise_of(result_of(response.clone(), error.clone())), "(async () => { try { const _r = await fetch($0); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "post", [Type::String, Type::Unknown], promise_of(result_of(response.clone(), error.clone())), "(async () => { try { const _r = await fetch($0, { method: \"POST\", body: JSON.stringify($1), headers: { \"Content-Type\": \"application/json\" } }); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "put", [Type::String, Type::Unknown], promise_of(result_of(response.clone(), error.clone())), "(async () => { try { const _r = await fetch($0, { method: \"PUT\", body: JSON.stringify($1), headers: { \"Content-Type\": \"application/json\" } }); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "delete", [Type::String], promise_of(result_of(response.clone(), error.clone())), "(async () => { try { const _r = await fetch($0, { method: \"DELETE\" }); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "json", [Type::Named("Response".to_string())], promise_of(result_of(Type::Unknown, error.clone())), "(async () => { try { const _r = await $0.json(); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "text", [Type::Named("Response".to_string())], promise_of(result_of(Type::String, error.clone())), "(async () => { try { const _r = await $0.text(); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
    ]);
}
