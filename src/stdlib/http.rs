use super::*;

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    fns.extend([
        stdlib_fn!("Http", "get", [Type::String], result_of(Type::Named("Response".to_string()), Type::Named("Error".to_string())), "(async () => { try { const _r = await fetch($0); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "post", [Type::String, Type::Unknown], result_of(Type::Named("Response".to_string()), Type::Named("Error".to_string())), "(async () => { try { const _r = await fetch($0, { method: \"POST\", body: JSON.stringify($1), headers: { \"Content-Type\": \"application/json\" } }); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "put", [Type::String, Type::Unknown], result_of(Type::Named("Response".to_string()), Type::Named("Error".to_string())), "(async () => { try { const _r = await fetch($0, { method: \"PUT\", body: JSON.stringify($1), headers: { \"Content-Type\": \"application/json\" } }); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "delete", [Type::String], result_of(Type::Named("Response".to_string()), Type::Named("Error".to_string())), "(async () => { try { const _r = await fetch($0, { method: \"DELETE\" }); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "json", [Type::Named("Response".to_string())], result_of(Type::Unknown, Type::Named("Error".to_string())), "(async () => { try { const _r = await $0.json(); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
        stdlib_fn!("Http", "text", [Type::Named("Response".to_string())], result_of(Type::String, Type::Named("Error".to_string())), "(async () => { try { const _r = await $0.text(); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })()"),
    ]);
}
