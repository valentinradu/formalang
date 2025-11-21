package tree_sitter_formalang_test

import (
	"testing"

	tree_sitter "github.com/smacker/go-tree-sitter"
	"github.com/tree-sitter/tree-sitter-formalang"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_formalang.Language())
	if language == nil {
		t.Errorf("Error loading Formalang grammar")
	}
}
