# MathContext bootstrap placement repair

The foundation job at c9d1c6e3 rejected the sequence loader: two original Suss
MathContext setup forms had been appended to a file reserved for complete pinned
upstream forms. Its retained source/license checks correctly failed. The original
CI log remains losslessly archived.

The licensed sequence loader is restored byte-for-byte to f36cc934. The same
MathContext declaration and getter installation now begin the original Suss
bootstrap source, at the same position in the combined program. Source/import
hashes and generated manifests are regenerated. form-equivalence.json proves
that every generated form, including setup order, is unchanged; only comments,
source placement and manifests differ. This source check does not substitute for
artifact execution or passing final-head CI.

All eight sequence provenance tests pass, including missing-notice and extra-form
rejection. All15 commands from the failed foundation step pass locally, including
364 Python tests, inventory, reader/catalog, source/import manifests, unchanged
provenance checks, numeric artifact, JVM array and suite-lock checks. Frozen input
hashes and terminal statuses are in foundation.json.gz. No guard, upstream
provenance or license criterion is weakened. The full workspace run on c9d1c6e3
continues in its separate unchanged checkout; promotion still requires complete
semantic acceptance, independent review and final-head CI.
