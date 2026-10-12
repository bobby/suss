// Copyright (c) Rich Hickey. All rights reserved.
// SPDX-License-Identifier: EPL-1.0
// Complete scalar-name munging branch and source tables retained under
// docs/compatibility/record-analyzer-context. The record emitter passes a
// resolved type-name symbol; analyzer-map munging is a separate interface.
// Executes compiled Suss with UTF-16 storage, not a host form evaluator.
pub(crate) const SOURCE: &str = r###"
(ns suss.internal.record-label)
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
(def ^:private reserved ["arguments" "abstract" "await" "boolean" "break" "byte" "case"
    "catch" "char" "class" "const" "continue"
    "debugger" "default" "delete" "do" "double"
    "else" "enum" "export" "extends" "final"
    "finally" "float" "for" "function" "goto" "if"
    "implements" "import" "in" "instanceof" "int"
    "interface" "let" "long" "native" "new"
    "package" "private" "protected" "public"
    "return" "short" "static" "super" "switch"
    "synchronized" "this" "throw" "throws"
    "transient" "try" "typeof" "var" "void"
    "volatile" "while" "with" "yield" "methods"
    "null" "constructor"])
(def ^:private replace-dots
  (fn [text]
    (loop [i 0 result ""]
      (if (< i (count text))
        (if (and (= (.charAt text i) ".") (< (+ i 1) (count text)) (= (.charAt text (+ i 1)) "."))
          (recur (+ i 2) (str result "_DOT__DOT_"))
          (recur (+ i 1) (str result (.charAt text i))))
        result))))
(def ^:private replace-division
  (fn [text]
    (loop [i 0 result ""]
      (if (< i (count text))
        (if (and (= (.charAt text i) "/") (< (+ i 1) (count text))
                 (let [unit (.charCodeAt text (+ i 1))]
                   (not (or (= unit 10) (= unit 13) (= unit 8232) (= unit 8233)))))
          (recur (+ i 2) (str result "." (.charAt text (+ i 1))))
          (recur (+ i 1) (str result (.charAt text i))))
        result))))
(def ^:private mark-reserved
  (fn [text]
    (loop [i 0 start 0 result ""]
      (if (< i (count text))
        (if (= (.charAt text i) ".")
          (let [part (suss.bootstrap/string-slice text start i)]
            (recur (+ i 1) (+ i 1) (str result part (if (some (fn [word] (= word part)) reserved) "$" "") ".")))
          (recur (+ i 1) start result))
        (let [part (suss.bootstrap/string-slice text start i)]
          (str result part (if (some (fn [word] (= word part)) reserved) "$" "")))))))
(def type-label
  (fn [value]
    (when-not (or (symbol? value) (string? value))
      (throw (suss.bootstrap/error "Record type label requires a symbol or string")))
    (let [text (mark-reserved (replace-division (replace-dots (str value))))]
      (loop [i 0 result ""]
        (if (< i (count text))
          (let [unit (.charAt text i)]
            (recur (+ i 1) (str result (get char-map unit unit))))
          result)))))
"###;
