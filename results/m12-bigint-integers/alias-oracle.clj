(assert (= "1.12.6" (clojure-version)))
(ns promote.alias (:refer-clojure :rename {inc' next-int dec' previous-int}))
(prn [(next-int 9223372036854775807) (previous-int -9223372036854775808)])
