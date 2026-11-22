export function unreachable(value: never): never {
  throw new Error(`a case the code does not know: ${JSON.stringify(value)}`);
}
