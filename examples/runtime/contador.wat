;; Contador: módulo de exemplo para o Exonet Runtime (spec/RUNTIME.md).
;;
;; Compile para WebAssembly binário com qualquer ferramenta, por exemplo:
;;   wasm-tools parse contador.wat -o contador.wasm
;; e publique:
;;   zero-wallet module-check --file contador.wasm
;;   zero-wallet module-publish --genesis G --node N --key K --file contador.wasm
;;
;; O módulo só enxerga o armazenamento da própria Comunidade. Não existe
;; função para ler ou mover saldos, votar, mexer no consenso ou em outra
;; Comunidade: essas operações simplesmente não fazem parte da interface.
(module
  (import "zero" "storage_get" (func $get (param i32 i32 i32 i32) (result i32)))
  (import "zero" "storage_set" (func $set (param i32 i32 i32 i32)))
  (import "zero" "output" (func $out (param i32 i32)))
  (memory (export "memory") 1)
  ;; chave "n" no endereço 0; valor (u64 little-endian) em 8..16
  (data (i32.const 0) "n")

  ;; incrementa o contador e devolve o novo valor
  (func (export "incrementar")
    (if (i32.eq (call $get (i32.const 0) (i32.const 1) (i32.const 8) (i32.const 8))
                (i32.const -1))
      (then (i64.store (i32.const 8) (i64.const 0))))
    (i64.store (i32.const 8) (i64.add (i64.load (i32.const 8)) (i64.const 1)))
    (call $set (i32.const 0) (i32.const 1) (i32.const 8) (i32.const 8))
    (call $out (i32.const 8) (i32.const 8)))

  ;; devolve o valor atual (use com `zero-wallet query`, sem custo)
  (func (export "ler")
    (drop (call $get (i32.const 0) (i32.const 1) (i32.const 8) (i32.const 8)))
    (call $out (i32.const 8) (i32.const 8))))
