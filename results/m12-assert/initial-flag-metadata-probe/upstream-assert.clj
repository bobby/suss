(defmacro assert
  "Evaluates expression x and throws an AssertionError with optional
  message if x does not evaluate to logical true.

  Assertion checks are omitted from compiled code if '*assert*' is
  false."
  {:added "1.0"}
  ([x]
     (when *assert*
       `(when-not ~x
          (throw (new AssertionError (str "Assert failed: " (pr-str '~x)))))))
  ([x message]
     (when *assert*
       `(when-not ~x
          (throw (new AssertionError (str "Assert failed: " ~message "\n" (pr-str '~x))))))))
