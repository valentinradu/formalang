; FormaLang code folding queries

; Definitions
(struct_definition) @fold
(trait_definition) @fold
(enum_definition) @fold
(module_definition) @fold
(default_block) @fold

; Control flow
(for_expression (block) @fold)
(if_expression (block) @fold)
(match_expression) @fold

; Blocks
(mount_block) @fold
(mount_children) @fold
(block) @fold

; Comments
(block_comment) @fold
