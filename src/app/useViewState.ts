import type { Dispatch, SetStateAction } from "react";
import { useApp } from "./AppContext";
/** Retain navigation state for the current profile without persisting private data. */
export function useViewState<T>(key: string, initial: T): [T, Dispatch<SetStateAction<T>>] {
  const { viewState, updateViewState } = useApp();
  const value = Object.prototype.hasOwnProperty.call(viewState, key)
    ? (viewState[key] as T)
    : initial;
  return [
    value,
    (next) =>
      updateViewState(key, (previous) => {
        const before = previous === undefined ? initial : (previous as T);
        return typeof next === "function" ? (next as (value: T) => T)(before) : next;
      }),
  ];
}
