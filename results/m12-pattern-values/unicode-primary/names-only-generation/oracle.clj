(import (quote java.lang.Character$UnicodeScript) (quote java.lang.Character$UnicodeBlock))
(assert (= "1.12.6" (clojure-version)))
(let [[names-file properties-file host-file] *command-line-args*]
  ;; These are observations of public Character semantics, not JDK source.
  (with-open [writer (clojure.java.io/writer names-file)]
    (dotimes [cp 0x110000]
      (when-let [name (Character/getName cp)]
        (when (= cp (Character/codePointOf name))
          (.write writer (str name "\n"))))))
  (let [scripts (Character$UnicodeScript/values)
        script-field (.getDeclaredField Character$UnicodeScript "aliases")
        _ (.setAccessible script-field true)
        aliases (keys (.get script-field nil))
        block-field (.getDeclaredField Character$UnicodeBlock "map")
        _ (.setAccessible block-field true)
        blocks (keys (.get block-field nil))
        categories ["L" "M" "N" "Z" "C" "P" "S" "LC" "Lu" "Ll" "Lt" "Lm" "Lo" "Mn" "Me" "Mc" "Nd" "Nl" "No" "Zs" "Zl" "Zp" "Cc" "Cf" "Co" "Cs" "Cn" "Pd" "Ps" "Pe" "Pc" "Po" "Pi" "Pf" "Sm" "Sc" "Sk" "So" "LD" "L1" "all" "ASCII"]
        posix ["Lower" "Upper" "ASCII" "Alpha" "Digit" "Alnum" "Punct" "Graph" "Print" "Blank" "Cntrl" "XDigit" "Space"]
        binary ["Alphabetic" "Ideographic" "Letter" "Lowercase" "Uppercase" "Titlecase" "Punctuation" "Control" "White_Space" "Digit" "Hex_Digit" "Join_Control" "Noncharacter_Code_Point" "Assigned" "ASCII" "ALPHA" "LOWER" "UPPER" "SPACE" "PUNCT" "XDIGIT" "ALNUM" "CNTRL" "GRAPH" "PRINT" "BLANK" "WORD"]
        java-props (for [m (.getMethods Character)
                         :when (and (.startsWith (.getName m) "is") (= Boolean/TYPE (.getReturnType m)) (= [Integer/TYPE] (vec (.getParameterTypes m))))]
                     (str "java" (subs (.getName m) 2)))
        valid? (fn [name] (try (java.util.regex.Pattern/compile (str "\\p{" name "}")) true (catch java.util.regex.PatternSyntaxException _ false)))
        accepted (fn [items prefix] (vec (sort (distinct (filter #(valid? (str prefix %)) items)))))
        folded (fn [items prefix] (vec (sort (distinct (for [name items :when (and (valid? (str prefix (.toLowerCase ^String name java.util.Locale/ROOT))) (valid? (str prefix (.toUpperCase ^String name java.util.Locale/ROOT))))] (.toUpperCase ^String name java.util.Locale/ROOT))))))
        properties {:plain (accepted (concat categories posix java-props) "")
                    :plain-folded (folded (concat categories posix java-props) "")
                    :is (accepted (concat categories posix binary) "Is")
                    :is-folded (folded (concat categories posix binary) "Is")
                    :scripts (vec (sort (distinct (concat (map str scripts) aliases))))
                    :blocks (vec (sort blocks))
                    :categories (accepted categories "gc=")}]
    (spit properties-file (pr-str properties)))
  (spit host-file (pr-str {:java-version (System/getProperty "java.version") :java-runtime (System/getProperty "java.runtime.version") :clojure-version (clojure-version)})))
