import { useState } from "react"

export function Counter() {
    const [count, setCount] = useState(0)

    return <div>
        <h1>Count</h1>
        <button onClick={setCount}>Increment</button>
    </div>
}
