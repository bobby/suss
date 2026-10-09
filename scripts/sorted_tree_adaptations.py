"""Enumerated whole-form adaptations for complete sorted collections authoring."""
import re
NAMES = ['StringIter', 'string-iter', 'ArrayIter', 'array-iter', 'INIT', 'START',
         'SeqIter', 'seq-iter', 'nil-iter', 'iter',
         'ES6Iterator', 'es6-iterator', 'ES6EntriesIterator', 'es6-entries-iterator',
         'ES6SetEntriesIterator', 'es6-set-entries-iterator',
         'tree-map-seq-push', 'PersistentTreeMapSeq', 'create-tree-map-seq',
         'tree-map-add', 'tree-map-append', 'tree-map-remove', 'tree-map-replace',
         'ValSeq', 'vals', 'PersistentTreeMap', 'PersistentTreeSet',
         'sorted-map', 'sorted-map-by', 'sorted-set', 'sorted-set-by',
         'take-while', 'mk-bound-fn', 'subseq', 'rsubseq']
CLASSES = ['StringIter', 'ArrayIter', 'SeqIter', 'ES6Iterator',
           'ES6EntriesIterator', 'ES6SetEntriesIterator', 'PersistentTreeMapSeq',
           'ValSeq', 'PersistentTreeMap', 'PersistentTreeSet']
DOSEQ = '''(loop [sorted-seq__ (seq coll) sorted-chunk__ nil sorted-count__ 0 sorted-i__ 0]
      (if (cljs.core/< sorted-i__ sorted-count__)
        (let [sorted-entry__ (suss.bootstrap/direct-protocol-call IIndexed -nth sorted-chunk__ sorted-i__)
              k (nth sorted-entry__ 0 nil) v (nth sorted-entry__ 1 nil)]
          (f v k)
          (recur sorted-seq__ sorted-chunk__ sorted-count__ (cljs.core/inc sorted-i__)))
        (when-let [sorted-seq__ (seq sorted-seq__)]
          (if (chunked-seq? sorted-seq__)
            (let [sorted-chunk-next__ (chunk-first sorted-seq__)]
              (recur (chunk-rest sorted-seq__) sorted-chunk-next__ (count sorted-chunk-next__) 0))
            (let [sorted-entry__ (first sorted-seq__)
                  k (nth sorted-entry__ 0 nil) v (nth sorted-entry__ 1 nil)]
              (f v k)
              (recur (next sorted-seq__) nil 0 0))))))'''

def adapt(original, name):
    result = original
    if result.startswith('(defn- ' + name + ' '):
        result = result.replace('(defn- ' + name + ' ', '(def ^:private ' + name + '\n  (fn ', 1) + ')'
    result = result.replace('(js/Error.', '(suss.bootstrap/error')
    result = result.replace('#js {:value #js [k v] :done false}', '(cljs.core/js-obj "value" (cljs.core/array k v) "done" false)')
    result = result.replace('#js {:value #js [x x] :done false}', '(cljs.core/js-obj "value" (cljs.core/array x x) "done" false)')
    result = result.replace('#js {:value x :done false}', '(cljs.core/js-obj "value" x "done" false)')
    result = result.replace('#js {:value nil :done true}', '(cljs.core/js-obj "value" nil "done" true)')
    result = result.replace('#js {}', '(cljs.core/js-obj)')
    # These are pinned call-site macro expansions, not changes to live public functions.
    for name2 in ['<', 'inc', 'dec', 'zero?', 'neg?', 'pos?', '==', 'aget', 'aset', 'array']:
        result = result.replace('(' + name2 + ' ', '(cljs.core/' + name2 + ' ')
    result = result.replace('(nil? ', '(suss.bootstrap/nil? ').replace('(string? ', '(suss.bootstrap/string? ')
    result = result.replace('(identical? ', '(cljs.core/== ')
    for hash_fn in ['hash-ordered-coll', 'hash-unordered-coll']:
        result = result.replace('(caching-hash coll ' + hash_fn + ' __hash)',
            '(let [cached-hash__ __hash]\n      (if (suss.bootstrap/nil? cached-hash__)\n        (let [computed-hash__ (' + hash_fn + ' coll)]\n          (set! __hash computed-hash__)\n          computed-hash__)\n        cached-hash__))')
    result = result.replace('(doseq [[k v] coll]\n      (f v k))', DOSEQ)
    if name in ['PersistentTreeMap', 'PersistentTreeSet', 'PersistentTreeMapSeq', 'ValSeq']:
        result = result.replace('(-equiv this other)', '(suss.bootstrap/direct-protocol-call IEquiv -equiv this other)')
    if name in ['PersistentTreeMap', 'PersistentTreeSet']:
        result = result.replace('(-lookup coll ', '(suss.bootstrap/direct-protocol-call ILookup -lookup coll ')
    if name == 'PersistentTreeMap':
        # ret/e and untyped entry nth remain public, as emitted by the pin.
        result = result.replace('(-assoc coll ', '(suss.bootstrap/direct-protocol-call IAssociative -assoc coll ')
    if name == 'ValSeq':
        result = result.replace('(-first mseq)', '(suss.bootstrap/direct-protocol-call ISeq -first mseq)')
        result = result.replace('(-next mseq)', '(suss.bootstrap/direct-protocol-call INext -next mseq)')
        result = result.replace('(-val me)', '(suss.bootstrap/direct-protocol-call IMapEntry -val me)')
    if name == 'PersistentTreeSet':
        # Pinned direct JS callback accepts three passed kv arguments; highest
        # shorthand argument is %2. Retain ignored accumulator/value semantics.
        result = result.replace('#(or (contains? other %2) (reduced false))',
                                '(fn [sorted-acc__ sorted-key__ sorted-value__] (or (contains? other sorted-key__) (reduced false)))')
        # Retain the genuine typed catch verbatim. Error ancestry is a blocker.
    if name == 'iter':
        result = result.replace('(str_ "Cannot create iterator from " coll)',
             '(let [sorted-text__ (str_ coll)] (.join (cljs.core/array "Cannot create iterator from " sorted-text__) ""))')
    return result
