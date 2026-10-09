# Array join evidence

Pinned ClojureScript revision: `c4295f303100bbf5afac449242d30bca1126f1a1`. Original reference fixture and 18 ordered case observations are retained under `evidence`; every observation is boolean true. The native test executes the same forms in runtime and macro phases, with collection before inspecting each result. The native log also records actual runtime OutOfFuel followed by callback-entry and cycle-marker recovery checks.

Coverage includes holes, separators, nested/self/mutual arrays, live later writes and shrink, overridden and noncallable join, Number/String constructors, million-hole sparse arrays, nested exceptions, future sparse insertion/removal, and growth beyond the captured length.

This proves these cases only. Inherited numeric indices, accessors, generic borrowed join, and full prototype semantics remain unsupported. Output is limited to 1,000,000 UTF-16 cells. Sparse traversal revalidates the chain and can be quadratic in the number of sparse entries. No full milestone or final-head CI claim follows from this evidence.

Additional native evidence rejects six host-forged cycle-stack corruptions as language errors after GC, then recovers after explicitly resetting the forged global. The async lifecycle test saves a nonnil caller join stack, enters a looping conversion, observes OutOfFuel and a secondary recovery interrupt, preserves the original trap and exact caller stack/frame, and retires without replay. Both tests passed; logs retained. Forged-root recovery does not claim automatic repair of corrupt caller state.
