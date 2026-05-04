use super::{StdlibFn, Type, promise_of, result_of, stdlib_fn, try_catch_async_result};

#[rustfmt::skip]
pub fn register(fns: &mut Vec<StdlibFn>) {
    let response = Type::Named("Response".to_string());
    let error = Type::Named("Error".to_string());

    fns.extend([
        stdlib_fn!("Http", "get",    [Type::String],                            promise_of(result_of(response.clone(), error.clone())), try_catch_async_result!("await fetch($0)")),
        stdlib_fn!("Http", "post",   [Type::String, Type::Unknown],             promise_of(result_of(response.clone(), error.clone())), try_catch_async_result!("await fetch($0, { method: \"POST\", body: JSON.stringify($1), headers: { \"Content-Type\": \"application/json\" } })")),
        stdlib_fn!("Http", "put",    [Type::String, Type::Unknown],             promise_of(result_of(response.clone(), error.clone())), try_catch_async_result!("await fetch($0, { method: \"PUT\", body: JSON.stringify($1), headers: { \"Content-Type\": \"application/json\" } })")),
        stdlib_fn!("Http", "delete", [Type::String],                            promise_of(result_of(response.clone(), error.clone())), try_catch_async_result!("await fetch($0, { method: \"DELETE\" })")),
        stdlib_fn!("Http", "json",   [Type::Named("Response".to_string())],     promise_of(result_of(Type::Unknown, error.clone())),    try_catch_async_result!("await $0.json()")),
        stdlib_fn!("Http", "text",   [Type::Named("Response".to_string())],     promise_of(result_of(Type::String, error.clone())),     try_catch_async_result!("await $0.text()")),
    ]);
}
