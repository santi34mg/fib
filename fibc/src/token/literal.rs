#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Integer(u64),
    Float(f32),
    Boolean(bool),
    Character(char),
    String(String),
    Null,
}
