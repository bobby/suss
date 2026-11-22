(assert (= "1.12.6" (clojure-version)))
(binding [*compile-path* "target/probe/m12-host-overload-classes"] (compile 'probe.overloads))
(doseq [file (sort-by str (file-seq (clojure.java.io/file "target/probe/m12-host-overload-classes")))
        :when (and (.isFile file) (.endsWith (.getName file) ".class"))]
  (let [reader (clojure.asm.ClassReader. (java.nio.file.Files/readAllBytes (.toPath file)))]
    (.accept reader
      (proxy [clojure.asm.ClassVisitor] [clojure.asm.Opcodes/ASM6]
        (visitMethod [access method-name descriptor signature exceptions]
          (when (= method-name "invokeStatic")
            (proxy [clojure.asm.MethodVisitor] [clojure.asm.Opcodes/ASM6]
              (visitMethodInsn [opcode owner name desc is-interface]
                (when (contains? #{"clojure/lang/Numbers" "clojure/lang/Util" "clojure/lang/Reflector" "clojure/lang/RT"} owner)
                  (prn (.getName file) owner name desc))))))) 0)))
