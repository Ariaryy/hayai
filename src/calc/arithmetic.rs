//! Hand-rolled recursive-descent arithmetic evaluator: `+ - * / % ^`,
//! parens, unary minus. Zero dependencies — this is the innermost, most
//! latency-sensitive layer of the calculator (every keystroke), so it stays
//! a plain float evaluator with no allocation beyond the input string.

struct Parser<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            chars: input.chars().peekable(),
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.chars.peek(), Some(c) if c.is_whitespace()) {
            self.chars.next();
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_ws();
        self.chars.peek().copied()
    }

    fn expr(&mut self) -> Option<f64> {
        let mut value = self.term()?;
        loop {
            match self.peek() {
                Some('+') => {
                    self.chars.next();
                    value += self.term()?;
                }
                Some('-') => {
                    self.chars.next();
                    value -= self.term()?;
                }
                _ => break,
            }
        }
        Some(value)
    }

    fn term(&mut self) -> Option<f64> {
        let mut value = self.power()?;
        loop {
            match self.peek() {
                Some('*') => {
                    self.chars.next();
                    value *= self.power()?;
                }
                Some('/') => {
                    self.chars.next();
                    let rhs = self.power()?;
                    if rhs == 0.0 {
                        return None;
                    }
                    value /= rhs;
                }
                Some('%') => {
                    self.chars.next();
                    let rhs = self.power()?;
                    if rhs == 0.0 {
                        return None;
                    }
                    value %= rhs;
                }
                _ => break,
            }
        }
        Some(value)
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.unary()?;
        if self.peek() == Some('^') {
            self.chars.next();
            let exponent = self.power()?; // right-associative
            return Some(base.powf(exponent));
        }
        Some(base)
    }

    fn unary(&mut self) -> Option<f64> {
        match self.peek() {
            Some('-') => {
                self.chars.next();
                Some(-self.unary()?)
            }
            Some('+') => {
                self.chars.next();
                self.unary()
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Option<f64> {
        match self.peek()? {
            '(' => {
                self.chars.next();
                let value = self.expr()?;
                if self.peek() != Some(')') {
                    return None;
                }
                self.chars.next();
                Some(value)
            }
            c if c.is_ascii_digit() || c == '.' => self.number(),
            _ => None,
        }
    }

    fn number(&mut self) -> Option<f64> {
        self.skip_ws();
        let mut text = String::new();
        while let Some(&c) = self.chars.peek() {
            if c.is_ascii_digit() || c == '.' {
                text.push(c);
                self.chars.next();
            } else {
                break;
            }
        }
        if text.is_empty() {
            return None;
        }
        text.parse().ok()
    }
}

/// Evaluates a plain arithmetic expression. Declines (returns `None`) inputs
/// with no operator at all — a bare number like `"7"` or `"2025"` must not
/// claim the query (it carries no computed value, and would otherwise
/// flicker a calc row while someone types an app name like "1password").
pub fn eval(input: &str) -> Option<f64> {
    if !input
        .chars()
        .any(|c| matches!(c, '+' | '-' | '*' | '/' | '%' | '^'))
    {
        return None;
    }
    let mut parser = Parser::new(input);
    let value = parser.expr()?;
    if parser.peek().is_some() {
        return None; // trailing garbage: not a full parse of the input
    }
    if !value.is_finite() {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_ops() {
        assert_eq!(eval("2+2"), Some(4.0));
        assert_eq!(eval("2 + 2 * 3"), Some(8.0));
        assert_eq!(eval("(2 + 2) * 3"), Some(12.0));
        assert_eq!(eval("10 / 4"), Some(2.5));
        assert_eq!(eval("10 % 3"), Some(1.0));
        assert_eq!(eval("2^10"), Some(1024.0));
    }

    #[test]
    fn unary_minus() {
        assert_eq!(eval("-5+10"), Some(5.0));
        assert_eq!(eval("3*-2"), Some(-6.0));
    }

    #[test]
    fn declines_bare_number() {
        assert_eq!(eval("7"), None);
        assert_eq!(eval("2025"), None);
    }

    #[test]
    fn declines_partial_parse() {
        assert_eq!(eval("2+2 hello"), None);
        assert_eq!(eval("1password"), None);
    }

    #[test]
    fn declines_division_by_zero() {
        assert_eq!(eval("1/0"), None);
    }
}
