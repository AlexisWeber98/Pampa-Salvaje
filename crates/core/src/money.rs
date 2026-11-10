//! La plata del protagonista, en pesos moneda nacional de 1919 con sus
//! centavos, contada en centavos para no perder ninguno al sumar.

use std::fmt;

/// Una suma de dinero, en centavos.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Money(pub u32);

impl Money {
    /// Nada.
    pub const ZERO: Self = Self(0);

    /// `pesos` pesos con `centavos` centavos.
    pub const fn new(pesos: u32, centavos: u32) -> Self {
        Self(pesos * 100 + centavos)
    }

    /// Los pesos enteros.
    pub const fn pesos(self) -> u32 {
        self.0 / 100
    }

    /// Los centavos sueltos, de 0 a 99.
    pub const fn centavos(self) -> u32 {
        self.0 % 100
    }

    /// Suma `amount`.
    pub fn earn(&mut self, amount: Self) {
        self.0 = self.0.saturating_add(amount.0);
    }

    /// Paga `amount` si alcanza, y dice si lo pagó; si no alcanza, no paga
    /// nada.
    pub fn spend(&mut self, amount: Self) -> bool {
        match self.0.checked_sub(amount.0) {
            Some(rest) => {
                self.0 = rest;
                true
            }
            None => false,
        }
    }
}

/// Como se escribía la plata: "$ 1,50", con coma decimal.
impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "$ {},{:02}", self.pesos(), self.centavos())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_is_written_in_pesos_with_a_decimal_comma() {
        assert_eq!(Money::new(1, 50).to_string(), "$ 1,50");
        assert_eq!(Money::new(5, 0).to_string(), "$ 5,00");
        assert_eq!(Money::new(0, 5).to_string(), "$ 0,05");
        assert_eq!(Money::ZERO.to_string(), "$ 0,00");
    }

    #[test]
    fn spending_needs_enough_and_otherwise_takes_nothing() {
        let mut wallet = Money::new(2, 0);
        wallet.earn(Money::new(1, 50));
        assert_eq!(wallet, Money::new(3, 50));
        assert!(wallet.spend(Money::new(3, 0)));
        assert_eq!(wallet, Money::new(0, 50));
        assert!(!wallet.spend(Money::new(0, 51)));
        assert_eq!(wallet, Money::new(0, 50), "nothing taken");
        assert!(wallet.spend(Money::new(0, 50)));
        assert_eq!(wallet, Money::ZERO);
    }
}
