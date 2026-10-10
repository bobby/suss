(ns suss-oracle.string-char-at (:require [suss-oracle.main :as transport]))
(def char-effects 0)
(defn -main [] (println (.stringify js/JSON (array
#js {:id "char-at-ascii-negative" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" -1)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-ascii-zero" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" 0)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-ascii-fraction" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" 1.5)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-ascii-one" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" 1)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-ascii-two" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" 2)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-ascii-three" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" 3)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-ascii-past" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" 4)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-ascii-huge" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" 4294967297)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-ascii-nan" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" ##NaN)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-ascii-infinity" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" ##Inf)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-ascii-nil" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" nil)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-ascii-false" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" false)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-ascii-string" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" "1.2")] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-utf16-negative" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "A\uD83D\uDE00Z" -1)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-utf16-zero" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "A\uD83D\uDE00Z" 0)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-utf16-fraction" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "A\uD83D\uDE00Z" 1.5)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-utf16-one" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "A\uD83D\uDE00Z" 1)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-utf16-two" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "A\uD83D\uDE00Z" 2)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-utf16-three" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "A\uD83D\uDE00Z" 3)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-utf16-past" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "A\uD83D\uDE00Z" 4)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-utf16-huge" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "A\uD83D\uDE00Z" 4294967297)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-utf16-nan" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "A\uD83D\uDE00Z" ##NaN)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-utf16-infinity" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "A\uD83D\uDE00Z" ##Inf)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-utf16-nil" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "A\uD83D\uDE00Z" nil)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-utf16-false" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "A\uD83D\uDE00Z" false)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-utf16-string" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "A\uD83D\uDE00Z" "1.2")] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-empty-negative" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "" -1)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-empty-zero" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "" 0)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-empty-fraction" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "" 1.5)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-empty-one" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "" 1)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-empty-two" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "" 2)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-empty-three" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "" 3)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-empty-past" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "" 4)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-empty-huge" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "" 4294967297)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-empty-nan" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "" ##NaN)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-empty-infinity" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "" ##Inf)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-empty-nil" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "" nil)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-empty-false" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "" false)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-empty-string" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "" "1.2")] (catch :default e [true e])) char-effects]))}
#js {:id "get-two-negative" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" -1)] (catch :default e [true e])) char-effects]))}
#js {:id "get-two-negative-half" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" -0.5)] (catch :default e [true e])) char-effects]))}
#js {:id "get-two-zero" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" 0)] (catch :default e [true e])) char-effects]))}
#js {:id "get-two-one" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" 1)] (catch :default e [true e])) char-effects]))}
#js {:id "get-two-past" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" 2)] (catch :default e [true e])) char-effects]))}
#js {:id "get-two-nil" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" nil)] (catch :default e [true e])) char-effects]))}
#js {:id "get-two-false" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" false)] (catch :default e [true e])) char-effects]))}
#js {:id "get-two-string" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" "1")] (catch :default e [true e])) char-effects]))}
#js {:id "get-two-huge" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" 4294967297)] (catch :default e [true e])) char-effects]))}
#js {:id "get-three-negative" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" -1 false)] (catch :default e [true e])) char-effects]))}
#js {:id "get-three-negative-half" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" -0.5 false)] (catch :default e [true e])) char-effects]))}
#js {:id "get-three-zero" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" 0 false)] (catch :default e [true e])) char-effects]))}
#js {:id "get-three-one" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" 1 false)] (catch :default e [true e])) char-effects]))}
#js {:id "get-three-past" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" 2 false)] (catch :default e [true e])) char-effects]))}
#js {:id "get-three-nil" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" nil false)] (catch :default e [true e])) char-effects]))}
#js {:id "get-three-false" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" false false)] (catch :default e [true e])) char-effects]))}
#js {:id "get-three-string" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" "1" false)] (catch :default e [true e])) char-effects]))}
#js {:id "get-three-huge" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" 4294967297 false)] (catch :default e [true e])) char-effects]))}
#js {:id "get-suite-13" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" 0)] (catch :default e [true e])) char-effects]))}
#js {:id "get-suite-28" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" 0 :not-found)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-object-index" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" (js-obj "valueOf" (fn [] (set! char-effects (+ (* char-effects 10) 1)) 1.5) "toString" (fn [] (set! char-effects (+ (* char-effects 10) 2)) "0")))] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-throw-index" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" (js-obj "valueOf" (fn [] (set! char-effects (+ (* char-effects 10) 1)) (throw 705)) "toString" (fn [] (set! char-effects (+ (* char-effects 10) 2)) "0")))] (catch :default e [true e])) char-effects]))}
#js {:id "get-two-object-index" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" (js-obj "valueOf" (fn [] (set! char-effects (+ (* char-effects 10) 1)) 1.5) "toString" (fn [] (set! char-effects (+ (* char-effects 10) 2)) "0")))] (catch :default e [true e])) char-effects]))}
#js {:id "get-two-throw-index" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" (js-obj "valueOf" (fn [] (set! char-effects (+ (* char-effects 10) 1)) (throw 705)) "toString" (fn [] (set! char-effects (+ (* char-effects 10) 2)) "0")))] (catch :default e [true e])) char-effects]))}
#js {:id "get-three-object-index" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" (js-obj "valueOf" (fn [] (set! char-effects (+ (* char-effects 10) 1)) 1.5) "toString" (fn [] (set! char-effects (+ (* char-effects 10) 2)) "0")) false)] (catch :default e [true e])) char-effects]))}
#js {:id "get-three-throw-index" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" (js-obj "valueOf" (fn [] (set! char-effects (+ (* char-effects 10) 1)) (throw 705)) "toString" (fn [] (set! char-effects (+ (* char-effects 10) 2)) "0")) false)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-default-index" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab")] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-extra-argument" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt "ab" 1 (do (set! char-effects (+ (* char-effects 10) 7)) 9))] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-ordered-receiver-index" :value (do (set! char-effects 0) (transport/encode [(try [false (.charAt (do (set! char-effects (+ (* char-effects 10) 1)) "ab") (do (set! char-effects (+ (* char-effects 10) 2)) 1))] (catch :default e [true e])) char-effects]))}
#js {:id "get-three-default-eager" :value (do (set! char-effects 0) (transport/encode [(try [false (get "ab" 0 (do (set! char-effects (+ (* char-effects 10) 3)) false))] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-borrow-number" :value (do (set! char-effects 0) (transport/encode [(try [false (.call (.-charAt "ab") 17 1)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-borrow-false" :value (do (set! char-effects 0) (transport/encode [(try [false (.call (.-charAt "ab") false 1)] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-borrow-object-ordered" :value (do (set! char-effects 0) (transport/encode [(try [false (.call (.-charAt "ab") (js-obj "toString" (fn [] (set! char-effects (+ (* char-effects 10) 2)) "ab") "valueOf" (fn [] (set! char-effects (+ (* char-effects 10) 1)) 17)) (js-obj "valueOf" (fn [] (set! char-effects (+ (* char-effects 10) 1)) 1.5) "toString" (fn [] (set! char-effects (+ (* char-effects 10) 2)) "0")))] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-borrow-receiver-throw" :value (do (set! char-effects 0) (transport/encode [(try [false (.call (.-charAt "ab") (js-obj "toString" (fn [] (set! char-effects (+ (* char-effects 10) 2)) (throw 706))) (js-obj "valueOf" (fn [] (set! char-effects (+ (* char-effects 10) 1)) 1.5) "toString" (fn [] (set! char-effects (+ (* char-effects 10) 2)) "0")))] (catch :default e [true e])) char-effects]))}
#js {:id "char-at-method-identity" :value (do (set! char-effects 0) (transport/encode [(try [false (identical? (.-charAt "a") (.-charAt "b"))] (catch :default e [true e])) char-effects]))}))))
(set! *main-cli-fn* -main)
