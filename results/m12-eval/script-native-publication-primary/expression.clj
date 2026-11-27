(assert (= "1.12.6" (clojure-version)))
(prn ["native-definition-after-later-prepared-input" (load-string "(eval '(def script-native-carried 42)) (eval 'script-native-carried)")])
(prn ["future-explicit-source-definition" (load-string "(def ^:dynamic script-future (fn [] 0)) (eval '(def ^:dynamic script-future (fn [] 41))) (def ^:dynamic script-future (fn [] 42)) (eval '(script-future))")])
(prn ["native-and-future-const-identities" (load-string "(eval '(def ^:const script-native-constant 99)) (def ^:const script-source-constant 42) [(eval 'script-native-constant) script-source-constant]")])
