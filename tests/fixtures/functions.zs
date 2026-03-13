export function add(a: number, b: number): number {
    return a + b
}

function greet(name: string, greeting: string = "Hello"): string {
    return `${greeting}, ${name}!`
}

const double = (n: number) => n * 2
