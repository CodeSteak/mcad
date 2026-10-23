// CodeMirror language support: Lezer grammar (scad.grammar) with highlighting tags.

import { HighlightStyle, LRLanguage, LanguageSupport, foldInside, foldNodeProp, indentNodeProp, continuedIndent, delimitedIndent, syntaxHighlighting } from "@codemirror/language";
import { styleTags, tags as t } from "@lezer/highlight";
import type { Tree } from "@lezer/common";
import { parser } from "./scad.grammar";

export const scadParser = parser.configure({
  props: [
    styleTags({
      "module function grid anchor include use": t.definitionKeyword,
      "if else for let each": t.controlKeyword,
      BooleanLiteral: t.bool,
      Undef: t.null,
      Number: t.number,
      String: t.string,
      IncludePath: t.string,
      LineComment: t.lineComment,
      BlockComment: t.blockComment,
      SpecialVariable: t.special(t.variableName),
      "DefName GridName AnchorName": t.definition(t.variableName),
      "ModuleDef/DefName FunctionDef/DefName": t.function(t.definition(t.variableName)),
      CallName: t.function(t.variableName),
      Label: t.labelName,
      AxisName: t.keyword,
      PlaneName: t.attributeName,
      PropertyName: t.propertyName,
      VariableName: t.variableName,
      Modifier: t.modifier,
      Wildcard: t.special(t.operator),
      "( )": t.paren,
      "[ ]": t.squareBracket,
      "{ }": t.brace,
      ", ;": t.separator,
      '"=" "?" ":" "." "+" "-" "/" "%" "^" "<" "<=" ">" ">=" "==" "!=" "&&" "||"': t.operator,
    }),
    indentNodeProp.add({
      Block: delimitedIndent({ closing: "}" }),
      GridDecl: delimitedIndent({ closing: "}" }),
      ArgList: delimitedIndent({ closing: ")" }),
      Vector: delimitedIndent({ closing: "]" }),
      "Instantiation IfStatement ForStatement LetStatement": continuedIndent(),
    }),
    foldNodeProp.add({ "Block GridDecl ArgList Vector": foldInside, BlockComment: (n) => ({ from: n.from + 2, to: n.to - 2 }) }),
  ],
});

export const scadLanguage = LRLanguage.define({
  name: "scad",
  parser: scadParser,
  languageData: { commentTokens: { line: "//", block: { open: "/*", close: "*/" } }, closeBrackets: { brackets: ["(", "[", "{", '"'] } },
});

export const scadHighlight = HighlightStyle.define([
  { tag: [t.definitionKeyword, t.controlKeyword, t.keyword], color: "var(--hl-keyword)", fontWeight: "600" },
  { tag: [t.number, t.bool, t.null], color: "var(--hl-number)" },
  { tag: t.string, color: "var(--hl-string)" },
  { tag: [t.lineComment, t.blockComment], color: "var(--hl-comment)", fontStyle: "italic" },
  { tag: t.special(t.variableName), color: "var(--hl-special)" },
  { tag: t.function(t.definition(t.variableName)), color: "var(--hl-def)", fontWeight: "600" },
  { tag: t.definition(t.variableName), color: "var(--hl-def)" },
  { tag: t.function(t.variableName), color: "var(--hl-call)" },
  { tag: t.labelName, color: "var(--hl-label)", fontWeight: "600" },
  { tag: t.attributeName, color: "var(--hl-plane)" },
  { tag: t.propertyName, color: "var(--hl-prop)" },
  { tag: t.modifier, color: "var(--hl-modifier)", fontWeight: "700" },
  { tag: t.special(t.operator), color: "var(--hl-modifier)", fontWeight: "700" },
  { tag: t.operator, color: "var(--hl-operator)" },
]);

export function scad(): LanguageSupport {
  return new LanguageSupport(scadLanguage, [syntaxHighlighting(scadHighlight)]);
}

/** Source ranges of statements prefixed with the `*` modifier (shown greyed out). */
export function disabledStatements(tree: Tree, doc: string): Array<{ from: number; to: number }> {
  const out: Array<{ from: number; to: number }> = [];
  tree.iterate({
    enter(n) {
      if (n.name !== "Instantiation") return;
      let c = n.node.firstChild;
      while (c && c.name === "Modifier") {
        if (doc.slice(c.from, c.to) === "*") {
          out.push({ from: n.from, to: n.to });
          return false;
        }
        c = c.nextSibling;
      }
      return undefined;
    },
  });
  return out;
}

/** Syntax error positions of the highlighting parse (used only by tests). */
export function errorNodes(tree: Tree): number[] {
  const out: number[] = [];
  tree.iterate({
    enter(n) {
      if (n.type.isError) out.push(n.from);
    },
  });
  return out;
}
