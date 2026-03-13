type Route =
    | Home
    | Profile(id: string)
    | NotFound

const page = match route {
    Home -> "home",
    Profile(id) -> id,
    NotFound -> "404",
}
