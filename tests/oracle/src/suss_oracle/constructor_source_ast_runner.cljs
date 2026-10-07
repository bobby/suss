(ns suss-oracle.constructor-source-ast-runner
  (:require-macros [suss-oracle.constructor-source-asts :refer [observe]]))

(deftype SourceConstructorPair [x y])
(deftype SourceConstructorEmpty [])
(def SourceRevisionConstructor 0)
(deftype ^{:declared true :tag false} SourceRevisionConstructor [x y])
(deftype ^js/Foreign SourceForeignHintConstructor [x y])
(deftype ^js/Number SourcePrimitiveHintConstructor [x y])
(def ^:private SourcePrivateConstructor 0)
(deftype SourcePrivateConstructor [x y])
(def effects [])
(defn mark [value] (do (set! effects (conj effects value)) value))
(def results
  [(let [copy (new SourceConstructorPair (mark 1) (mark 2))]
     [(observe "explicit-new" copy) [(.-x copy) (.-y copy)]])
   (let [copy (SourceConstructorPair. (mark 3) (mark 4))]
     [(observe "shorthand-new" copy) [(.-x copy) (.-y copy)]])
   (let [ctor SourceConstructorPair copy (new ctor (mark 5) (mark 6))]
     [(observe "local-new" copy) [(.-x copy) (.-y copy)]])
   (let [copy (new SourceConstructorEmpty)] [(observe "empty-new" copy) []])
   (let [copy (new suss-oracle.constructor-source-ast-runner/SourceConstructorPair (mark 7) (mark 8))]
     [(observe "qualified-new" copy) [(.-x copy) (.-y copy)]])
   (let [copy (new SourceRevisionConstructor 9 10)]
     [(observe "current-type-metadata" copy) [(.-x copy) (.-y copy)]])
   (let [copy (new SourceForeignHintConstructor 11 12)]
     [(observe "foreign-source-hint" copy) [(.-x copy) (.-y copy)]])
   (let [copy (new SourcePrimitiveHintConstructor 13 14)]
     [(observe "primitive-source-hint" copy) [(.-x copy) (.-y copy)]])
   (let [copy (new SourcePrivateConstructor 15 16)]
     [(observe "preserved-type-metadata" copy) [(.-x copy) (.-y copy)]])])
(defn -main []
  (println (.stringify js/JSON (clj->js [results effects]))))
(set! *main-cli-fn* -main)
