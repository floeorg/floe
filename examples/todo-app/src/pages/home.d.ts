import { useState } from "react";
import { v4 } from "uuid";
import { type Todo, type Filter, type Validation } from "../types";
import { validate, filterBy, remaining, stats, search } from "../todo";
export declare function HomePage(): JSX.Element;
