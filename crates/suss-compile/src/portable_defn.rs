// Copyright (c) Rich Hickey. All rights reserved.
// The use and distribution terms for this software are covered by the
// Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php).
// By using this software in any fashion, you agree to be bound by its terms.
// You must not remove this notice, or any other, from this software.
// SPDX-License-Identifier: EPL-1.0
// Adapted from pinned cljs/core.cljc defn, lines 3364–3434.
// See docs/compatibility/compiled-defn-provenance.json and runtime/core-import/epl-v10.html.
// Native fn arity dispatch replaces JavaScript function properties. Expansion
// executes in the compiled Macro Store; this is not a Rust macro evaluator.
pub(crate) const SOURCE: &str = r#"
(defmacro defn [name & declarations]
  (if (symbol? name) nil
    (throw (ex-info "First argument to defn must be a symbol" {})))
  (let [attributes (if (string? (first declarations)) {:doc (first declarations)} {})
        declarations (if (string? (first declarations)) (next declarations) declarations)
        attributes (if (map? (first declarations)) (conj attributes (first declarations)) attributes)
        declarations (if (map? (first declarations)) (next declarations) declarations)
        declarations (if (vector? (first declarations)) (list declarations) declarations)
        parsed (loop [remaining declarations signatures [] attributes attributes]
                 (if (seq remaining)
                   (if (map? (first remaining))
                     (if (next remaining)
                       (throw (ex-info "defn trailing attributes must follow all signatures" {}))
                       [signatures (conj attributes (first remaining))])
                     (recur (next remaining) (conj signatures (first remaining)) attributes))
                   [signatures attributes]))
        signatures (nth parsed 0)
        attributes (nth parsed 1)
        arglists (loop [remaining (seq signatures) arguments []]
                   (if remaining
                     (recur (next remaining) (conj arguments (first (first remaining))))
                     (apply list arguments)))
        attributes (conj {:arglists (list 'quote arglists)} attributes)
        attributes (conj (if (meta name) (meta name) {}) attributes)]
    (if (seq signatures) nil
      (throw (ex-info "defn requires at least one signature" {})))
    (list 'def (with-meta name attributes) (cons 'suss.core/fn (seq signatures)))))
"#;
