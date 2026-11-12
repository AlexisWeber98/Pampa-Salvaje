//! Lo que venden el pulpero y el almacenero, a qué precio, y cómo se les
//! compra: de a una cosa, con la plata del protagonista, si sabe comerciar
//! ([`Ability::Trade`]) y, para las armas, si sabe usarlas
//! ([`Ability::Weapons`]). Los precios son en pesos moneda nacional de 1919: un
//! peón ganaba de $ 1,50 a $ 2 por día.

use crate::abilities::{Abilities, Ability};
use crate::items::{Item, Items};
use crate::money::Money;

/// Un local que vende: la pulpería (bebida, vicios y algo para comer) o el
/// almacén de ramos generales (víveres, herramientas, ropa y armas).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shop {
    Pulperia,
    Almacen,
}

/// Algo que se vende: la cosa, su precio y la habilidad que hace falta, además
/// de comerciar, para comprarla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ware {
    pub item: Item,
    pub price: Money,
    pub needs: Option<Ability>,
}

const fn ware(item: Item, pesos: u32, centavos: u32) -> Ware {
    Ware {
        item,
        price: Money::new(pesos, centavos),
        needs: None,
    }
}

const fn weapon(item: Item, pesos: u32, centavos: u32) -> Ware {
    Ware {
        item,
        price: Money::new(pesos, centavos),
        needs: Some(Ability::Weapons),
    }
}

const PULPERIA: [Ware; 7] = [
    ware(Item::CaneGlass, 0, 10),
    ware(Item::GinGlass, 0, 15),
    ware(Item::WineGlass, 0, 10),
    ware(Item::CaneBottle, 0, 80),
    ware(Item::Tobacco, 0, 25),
    ware(Item::Matches, 0, 5),
    ware(Item::Biscuit, 0, 10),
];

const ALMACEN: [Ware; 15] = [
    ware(Item::Yerba, 0, 80),
    ware(Item::Sugar, 0, 40),
    ware(Item::Jerky, 0, 60),
    ware(Item::Flour, 0, 20),
    ware(Item::Candles, 0, 5),
    ware(Item::Knife, 3, 0),
    ware(Item::Axe, 4, 0),
    ware(Item::Shovel, 3, 0),
    ware(Item::Maneador, 2, 50),
    ware(Item::Lasso, 4, 0),
    ware(Item::Alpargatas, 1, 20),
    ware(Item::Poncho, 8, 0),
    weapon(Item::Revolver, 25, 0),
    weapon(Item::Winchester, 60, 0),
    weapon(Item::Cartridges, 2, 50),
];

/// Lo que dice quien atiende cuando el protagonista todavía no sabe
/// comerciar.
pub const NOT_YET: &str = "Volvé cuando tengas con qué.";

impl Shop {
    /// Lo que vende, en el orden en que lo ofrece.
    pub fn wares(self) -> &'static [Ware] {
        match self {
            Self::Pulperia => &PULPERIA,
            Self::Almacen => &ALMACEN,
        }
    }

    /// Cómo saluda quien atiende cuando el protagonista se acerca.
    pub fn greeting(self) -> &'static str {
        match self {
            Self::Pulperia => "Buenas. ¿Qué le sirvo?",
            Self::Almacen => "Buenas. ¿Qué anda necesitando?",
        }
    }
}

/// Por qué no se pudo comprar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Refusal {
    /// El protagonista todavía no sabe comerciar ([`Ability::Trade`]).
    NoTrade,
    /// Para esto hace falta otra habilidad (las armas, [`Ability::Weapons`]).
    Needs(Ability),
    /// No le alcanza la plata.
    Short,
}

impl Ware {
    /// Si el protagonista, con `abilities` y `money`, puede comprarlo ahora, o
    /// por qué no.
    pub fn check(&self, abilities: Abilities, money: Money) -> Result<(), Refusal> {
        if !abilities.has(Ability::Trade) {
            return Err(Refusal::NoTrade);
        }
        if let Some(ability) = self.needs.filter(|&a| !abilities.has(a)) {
            return Err(Refusal::Needs(ability));
        }
        if money < self.price {
            return Err(Refusal::Short);
        }
        Ok(())
    }

    /// Lo compra una vez: paga su precio y se lo lleva. Si no puede
    /// ([`Ware::check`]), no paga ni se lleva nada.
    pub fn buy(
        &self,
        abilities: Abilities,
        money: &mut Money,
        items: &mut Items,
    ) -> Result<(), Refusal> {
        self.check(abilities, *money)?;
        money.spend(self.price);
        items.receive(self.item);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn priced(shop: Shop) -> Vec<(&'static str, String)> {
        shop.wares()
            .iter()
            .map(|w| (w.item.name(), w.price.to_string()))
            .collect()
    }

    #[test]
    fn the_pulperia_sells_drink_and_smokes_and_the_almacen_the_rest_at_1919_prices() {
        assert_eq!(
            priced(Shop::Pulperia),
            [
                ("Vaso de caña", "$ 0,10"),
                ("Vaso de ginebra", "$ 0,15"),
                ("Vaso de vino", "$ 0,10"),
                ("Botella de caña", "$ 0,80"),
                ("Tabaco y papel de armar", "$ 0,25"),
                ("Fósforos", "$ 0,05"),
                ("Galleta", "$ 0,10"),
            ]
            .map(|(n, p)| (n, p.to_string()))
        );
        assert_eq!(
            priced(Shop::Almacen),
            [
                ("Yerba (1 kg)", "$ 0,80"),
                ("Azúcar (1 kg)", "$ 0,40"),
                ("Charqui (1 kg)", "$ 0,60"),
                ("Harina (1 kg)", "$ 0,20"),
                ("Velas", "$ 0,05"),
                ("Cuchillo", "$ 3,00"),
                ("Hacha", "$ 4,00"),
                ("Pala", "$ 3,00"),
                ("Maneador", "$ 2,50"),
                ("Lazo", "$ 4,00"),
                ("Alpargatas", "$ 1,20"),
                ("Poncho", "$ 8,00"),
                ("Revólver", "$ 25,00"),
                ("Winchester", "$ 60,00"),
                ("Caja de 25 balas", "$ 2,50"),
            ]
            .map(|(n, p)| (n, p.to_string()))
        );
        // Solo las armas y sus balas piden saber usar armas.
        for shop in [Shop::Pulperia, Shop::Almacen] {
            for w in shop.wares() {
                let arms = matches!(w.item, Item::Revolver | Item::Winchester | Item::Cartridges);
                assert_eq!(w.needs, arms.then_some(Ability::Weapons), "{:?}", w.item);
            }
        }
    }

    #[test]
    fn buying_pays_the_price_and_carries_one_more_unless_something_is_missing() {
        let mut abilities = Abilities::default();
        let mut money = Money::new(3, 0);
        let mut items = Items::default();
        let cane = Shop::Pulperia.wares()[0];
        let revolver = Shop::Almacen.wares()[12];
        assert_eq!(revolver.item, Item::Revolver);

        // Sin saber comerciar, nada cambia.
        assert_eq!(cane.buy(abilities, &mut money, &mut items), Err(Refusal::NoTrade));
        assert_eq!((money, items.count(Item::CaneGlass)), (Money::new(3, 0), 0));

        abilities.unlock(Ability::Trade);
        for left in [290, 280] {
            assert_eq!(cane.buy(abilities, &mut money, &mut items), Ok(()));
            assert_eq!(money, Money(left));
        }
        assert_eq!(items.count(Item::CaneGlass), 2);

        // Las armas piden saber usarlas; y la plata tiene que alcanzar.
        assert_eq!(
            revolver.buy(abilities, &mut money, &mut items),
            Err(Refusal::Needs(Ability::Weapons))
        );
        abilities.unlock(Ability::Weapons);
        assert_eq!(revolver.buy(abilities, &mut money, &mut items), Err(Refusal::Short));
        assert_eq!((money, items.count(Item::Revolver)), (Money(280), 0));
        assert_eq!(revolver.check(abilities, Money::new(25, 0)), Ok(()));
    }
}
