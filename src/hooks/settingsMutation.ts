interface OptimisticMutationOptions<T> {
  previous: T;
  optimistic: T;
  apply: (value: T) => void;
  mutate: () => Promise<T>;
}

export async function runOptimisticMutation<T>({
  previous,
  optimistic,
  apply,
  mutate,
}: OptimisticMutationOptions<T>): Promise<T> {
  apply(optimistic);
  try {
    const resolved = await mutate();
    apply(resolved);
    return resolved;
  } catch (error) {
    apply(previous);
    throw error;
  }
}
