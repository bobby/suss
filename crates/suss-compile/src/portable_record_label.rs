// Copyright (c) Rich Hickey. All rights reserved.
// SPDX-License-Identifier: EPL-1.0
// Complete scalar-name munging branch and source tables retained under
// docs/compatibility/record-analyzer-context. The record emitter passes a
// resolved type-name symbol; analyzer-map munging is a separate interface.
// Executes compiled Suss with UTF-16 storage, not a host form evaluator.
pub(crate) const SOURCE: &str = r###"

(ns suss.internal.record-label)
;; Private joins receive only known strings. Evaluate all operands left-to-right
;; before concatenation; preserve exact UTF-16 units without source coercion.
(def ^:private label-join3 (fn [a b c] (suss.bootstrap/concat-string (suss.bootstrap/concat-string a b) c)))
(def ^:private label-join4 (fn [a b c d] (suss.bootstrap/concat-string (suss.bootstrap/concat-string (suss.bootstrap/concat-string a b) c) d)))

(def ^:private char-map (array-map "-"    "_"
       ":"    "_COLON_"
       "+"    "_PLUS_"
       ">"    "_GT_"
       "<"    "_LT_"
       "="    "_EQ_"
       "~"    "_TILDE_"
       "!"    "_BANG_"
       "@"    "_CIRCA_"
       "#"    "_SHARP_"
       "'"    "_SINGLEQUOTE_"
       "\\\"" "_DOUBLEQUOTE_"
       "%"    "_PERCENT_"
       "^"    "_CARET_"
       "&"    "_AMPERSAND_"
       "*"    "_STAR_"
       "|"    "_BAR_"
       "{"    "_LBRACE_"
       "}"    "_RBRACE_"
       "["    "_LBRACK_"
       "]"    "_RBRACK_"
       "/"    "_SLASH_"
       "\\\\" "_BSLASH_"
       "?"    "_QMARK_"))
(def ^:private reserved ["abstract" "arguments" "await" "boolean" "break" "byte" "case" "catch" "char" "class" "const" "constructor" "continue" "debugger" "default" "delete" "do" "double" "else" "enum" "export" "extends" "final" "finally" "float" "for" "function" "goto" "if" "implements" "import" "in" "instanceof" "int" "interface" "let" "long" "methods" "native" "new" "null" "package" "private" "protected" "public" "return" "short" "static" "super" "switch" "synchronized" "this" "throw" "throws" "transient" "try" "typeof" "var" "void" "volatile" "while" "with" "yield"])

(def ^:private reserved-word?
  (fn [part]
    ;; The complete immutable reserved table is sorted above. Binary search
    ;; preserves exact string membership without eager hash-table construction.
    (loop [lo 0 hi (count reserved)]
      (if (< lo hi)
        (let [mid (quot (+ lo hi) 2)
              order (compare part (nth reserved mid))]
          (if (zero? order)
            true
            (if (< order 0)
              (recur lo mid)
              (recur (inc mid) hi))))
        false))))
(def ^:private munge-unit
  (fn [unit]
    ;; Complete constant character table, compiled into literal comparisons.
    (case unit
      "-" "_"
      ":" "_COLON_"
      "+" "_PLUS_"
      ">" "_GT_"
      "<" "_LT_"
      "=" "_EQ_"
      "~" "_TILDE_"
      "!" "_BANG_"
      "@" "_CIRCA_"
      "#" "_SHARP_"
      "'" "_SINGLEQUOTE_"
      "\\\"" "_DOUBLEQUOTE_"
      "%" "_PERCENT_"
      "^" "_CARET_"
      "&" "_AMPERSAND_"
      "*" "_STAR_"
      "|" "_BAR_"
      "{" "_LBRACE_"
      "}" "_RBRACE_"
      "[" "_LBRACK_"
      "]" "_RBRACK_"
      "/" "_SLASH_"
      "\\\\" "_BSLASH_"
      "?" "_QMARK_"
      unit)))
(def ^:private replace-dots
  (fn [text]
    (let [length (count text)] (if (< (suss.bootstrap/string-index-of text "..") 0) text (loop [i 0 result ""]
      (if (< i length)
        (if (and (= (.charAt text i) ".") (< (+ i 1) length) (= (.charAt text (+ i 1)) "."))
          (recur (+ i 2) (suss.bootstrap/concat-string result "_DOT__DOT_"))
          (recur (+ i 1) (suss.bootstrap/concat-string result (.charAt text i))))
        result))))))
(def ^:private replace-division
  (fn [text]
    (let [length (count text)]
      (loop [start 0 result ""]
        (let [suffix (suss.bootstrap/string-slice text start length)
              relative (suss.bootstrap/string-index-of suffix "/")]
          (if (< relative 0)
            (suss.bootstrap/concat-string result suffix)
            (let [slash (+ start relative)
                  next-index (+ slash 1)]
              (if (and (< next-index length)
                       (let [unit (.charCodeAt text next-index)]
                         (not (or (= unit 10) (= unit 13)
                                  (= unit 8232) (= unit 8233)))))
                (recur (+ slash 2)
                       (label-join4 result
                                    (suss.bootstrap/string-slice text start slash)
                                    "."
                                    (.charAt text next-index)))
                (recur next-index
                       (suss.bootstrap/concat-string
                         result
                         (suss.bootstrap/string-slice text start next-index)))))))))))

(def ^:private mark-reserved
  (fn [text]
    (let [length (count text)]
      (loop [start 0 result ""]
        (let [suffix (suss.bootstrap/string-slice text start length)
              relative (suss.bootstrap/string-index-of suffix ".")]
          (if (< relative 0)
            (let [part (suss.bootstrap/string-slice text start length)]
              (label-join3 result part (if (reserved-word? part) "$" "")))
            (let [dot (+ start relative)
                  part (suss.bootstrap/string-slice text start dot)]
              (recur (+ dot 1)
                     (label-join4 result part
                                  (if (reserved-word? part) "$" "")
                                  ".")))))))))
(def type-label
  (fn [value]
    (when-not (or (symbol? value) (string? value))
      (throw (suss.bootstrap/error "Record type label requires a symbol or string")))
    (let [text (mark-reserved (replace-division (replace-dots (str value))))
          length (count text)]
      (loop [i 0 result ""]
        (if (< i length)
          (let [unit (.charAt text i)
                code (.charCodeAt text i)
                unchanged (or (and (<= 65 code) (<= code 90))
                              (and (<= 97 code) (<= code 122))
                              (and (<= 48 code) (<= code 57))
                              (= code 95) (= code 46) (= code 36))]
            (recur (+ i 1) (suss.bootstrap/concat-string result
                              (if unchanged unit (munge-unit unit)))))
          result)))))

"###;
