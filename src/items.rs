use super::*;
use std::cell::RefCell;
use std::rc::Rc;

use piet_common::{Piet, RenderContext};

pub type DrawResult = Result<(), piet_common::Error>;

#[derive(Default)]
pub struct Geometry<'a> {
    pub x: Property<'a, f64>,
    pub y: Property<'a, f64>,
    pub width: Property<'a, f64>,
    pub height: Property<'a, f64>,
}
impl<'a> Geometry<'a> {
    pub fn width(&self) -> f64 {
        self.width.get()
    }
    pub fn height(&self) -> f64 {
        self.height.get()
    }
    pub fn left(&self) -> f64 {
        self.x.get()
    }
    pub fn top(&self) -> f64 {
        self.y.get()
    }
    pub fn right(&self) -> f64 {
        self.x.get() + self.width.get()
    }
    pub fn bottom(&self) -> f64 {
        self.y.get() + self.height.get()
    }
    pub fn vertical_center(&self) -> f64 {
        self.x.get() + self.width.get() / 2.
    }
    pub fn horizontal_center(&self) -> f64 {
        self.y.get() + self.height.get() / 2.
    }

    pub fn to_rect(&self) -> piet_common::kurbo::Rect {
        piet_common::kurbo::Rect::new(self.left(), self.top(), self.right(), self.bottom())
    }
}
/*
enum SizePolicy {
    Fixed(f64),
    Minimum(f64),
    Maximum(f64)
}*/

pub struct LayoutInfo<'a> {
    pub preferred_width: Property<'a, f64>,
    pub preferred_height: Property<'a, f64>,
    pub maximum_width: Property<'a, f64>,
    pub maximum_height: Property<'a, f64>,
    pub minimum_width: Property<'a, f64>,
    pub minimum_height: Property<'a, f64>,
}
impl<'a> Default for LayoutInfo<'a> {
    fn default() -> Self {
        LayoutInfo {
            preferred_width: 0.0.into(),
            preferred_height: 0.0.into(),
            maximum_height: std::f64::MAX.into(),
            maximum_width: std::f64::MAX.into(),
            minimum_width: 0.0.into(),
            minimum_height: 0.0.into(),
        }
    }
}

type QPointF = piet_common::kurbo::Point;

#[derive(Clone, Copy)]
pub enum MouseEvent {
    Press(QPointF),
    Release(QPointF),
    Move(QPointF),
}
impl MouseEvent {
    fn position_ref(&mut self) -> &mut QPointF {
        match self {
            MouseEvent::Press(ref mut x) => x,
            MouseEvent::Release(ref mut x) => x,
            MouseEvent::Move(ref mut x) => x,
        }
    }

    pub fn position(mut self) -> QPointF {
        *self.position_ref()
    }

    pub fn translated(mut self, translation: QPointF) -> MouseEvent {
        {
            let pos = self.position_ref();
            *pos += translation.to_vec2();
        }
        self
    }
}

pub trait Item<'a> {
    fn geometry(&self) -> &Geometry<'a>;
    fn layout_info(&self) -> &LayoutInfo<'a>;
    fn draw(&self, _rc: &mut Piet) -> DrawResult { Ok(()) }
    fn mouse_event(&self, _event: MouseEvent) -> bool {
        false
    }
}

pub trait ItemContainer<'a> {
    fn add_child(&self, child: Rc<dyn Item<'a> + 'a>);
}

impl<'a, T, I: Item<'a> + 'a> Item<'a> for T
where
    T: ::std::ops::Deref<Target = I>,
{
    fn geometry(&self) -> &Geometry<'a> {
        ::std::ops::Deref::deref(self).geometry()
    }
    fn layout_info(&self) -> &LayoutInfo<'a> {
        ::std::ops::Deref::deref(self).layout_info()
    }
    fn draw(&self, rc: &mut Piet) -> DrawResult {
        ::std::ops::Deref::deref(self).draw(rc)
    }
    fn mouse_event(&self, event: MouseEvent) -> bool {
        ::std::ops::Deref::deref(self).mouse_event(event)
    }
}

mod layout_engine {

    use std::ops::Add;

    pub type Coord = f64;

    #[derive(Default)]
    pub struct ItemInfo {
        pub min: Coord,
        pub max: Coord,
        pub preferred: Coord,
        pub expand: usize,
    }

    impl<'a> Add<&'a ItemInfo> for ItemInfo {
        type Output = ItemInfo;

        fn add(self, other: &'a ItemInfo) -> ItemInfo {
            ItemInfo {
                min: self.min + other.min,
                max: self.max + other.max, // the idea is that it saturate with the max value or infinity
                preferred: self.preferred + other.preferred,
                expand: self.expand + other.expand,
            }
        }
    }

    pub fn compute_total_info(info: &[ItemInfo], spacing: Coord) -> ItemInfo {
        let mut sum: ItemInfo = info.iter().fold(ItemInfo::default(), Add::add);
        let total_spacing = spacing * (info.len() - 1) as Coord;
        sum.min += total_spacing;
        sum.max += total_spacing;
        sum.preferred += total_spacing;
        sum
    }

    #[derive(Clone, Copy)]
    pub struct ItemResult {
        pub size: Coord,
        pub pos: Coord,
    }

    pub fn do_layout(
        info: &[ItemInfo],
        total: ItemInfo,
        spacing: Coord,
        size: Coord,
    ) -> Vec<ItemResult> {
        // FIXME! consider maximum, or the case where we are smaller that the minimum
        if size < total.preferred {
            let to_remove = total.preferred - size;
            let total_allowed_to_remove = total.preferred - total.min;

            let mut pos = 0 as Coord;
            info.iter()
                .map(|it| {
                    let s = it.preferred
                        - (it.preferred - it.min) * to_remove / total_allowed_to_remove;
                    let p = pos;
                    pos += s + spacing;
                    ItemResult { size: s, pos: p }
                })
                .collect()
        } else {
            let to_add = size - total.preferred;
            //let total_allowed_to_add = total.max - preferred;

            let mut pos = 0 as Coord;
            info.iter()
                .map(|it| {
                    let s = if total.expand > 0 {
                        it.preferred + to_add * it.expand as Coord / total.expand as Coord
                    } else {
                        it.preferred + to_add / info.len() as Coord
                    };
                    let p = pos;
                    pos += s + spacing;
                    ItemResult { size: s, pos: p }
                })
                .collect()
        }
    }

}

macro_rules! declare_box_layout {
    ($ColumnLayout:ident, $x:ident, $width:ident, $minimum_width:ident, $maximum_width:ident, $preferred_width:ident,
        $y:ident, $height:ident, $minimum_height:ident, $maximum_height:ident, $preferred_height:ident) => {
        #[derive(Default)]
        pub struct $ColumnLayout<'a> {
            pub geometry: Geometry<'a>,
            pub layout_info: LayoutInfo<'a>,
            pub spacing: Property<'a, f64>,

            children: RefCell<Vec<Rc<dyn Item<'a> + 'a>>>,
            positions: Property<'a, Vec<layout_engine::ItemResult>>,
        }
        impl<'a> Item<'a> for $ColumnLayout<'a> {
            fn geometry(&self) -> &Geometry<'a> {
                &self.geometry
            }
            fn layout_info(&self) -> &LayoutInfo<'a> {
                &self.layout_info
            }
            fn draw(&self, rc: &mut Piet) -> DrawResult {
                let g = self.geometry().to_rect();
                rc.with_save(|rc| {
                    rc.transform(piet_common::kurbo::Affine::translate(g.origin().to_vec2()));
                    for child in self.children.borrow().iter() {
                        child.draw(rc)?
                    }
                    Ok(())
                })
            }


            fn mouse_event(&self, event: MouseEvent) -> bool {
                for i in self.children.borrow().iter() {
                    let g = i.geometry().to_rect();
                    if g.contains(event.position()) {
                        return i.mouse_event(event.translated(g.origin()));
                    }
                }
                return false;
            }
        }

        impl<'a> ItemContainer<'a> for Rc<$ColumnLayout<'a>> {
            fn add_child(&self, child: Rc<dyn Item<'a> + 'a>) {
                self.children.borrow_mut().push(child);
                $ColumnLayout::build_layout(self);
            }
        }

        impl<'a> $ColumnLayout<'a> {
            pub fn new() -> Rc<Self> {
                Default::default()
            }

            fn build_layout(this: &Rc<Self>) {
                // The minimum width is the max of the minimums
                let w = Rc::downgrade(this);
                this.layout_info.$minimum_width.set_binding(move || {
                    w.upgrade().map_or(0., |x| {
                        x.children
                            .borrow()
                            .iter()
                            .map(|i| i.layout_info().$minimum_width.get())
                            .fold(0., f64::max)
                    })
                });

                // The minimum height is the sum of the minimums
                let w = Rc::downgrade(this);
                this.layout_info.$minimum_height.set_binding(move || {
                    w.upgrade().map_or(0., |x| {
                        x.children
                            .borrow()
                            .iter()
                            .map(|i| i.layout_info().$minimum_height.get())
                            .sum()
                    })
                });

                // The maximum width is the min of the maximums
                let w = Rc::downgrade(this);
                this.layout_info.$maximum_width.set_binding(move || {
                    w.upgrade().map_or(0., |x| {
                        x.children
                            .borrow()
                            .iter()
                            .map(|i| i.layout_info().$maximum_width.get())
                            .fold(std::f64::MAX, f64::min)
                    })
                });
                // The maximum height is the sum of the maximums (assume it saturates)
                let w = Rc::downgrade(this);
                this.layout_info.$maximum_height.set_binding(move || {
                    w.upgrade().map_or(0., |x| {
                        x.children
                            .borrow()
                            .iter()
                            .map(|i| i.layout_info().$maximum_height.get())
                            .sum()
                    })
                });

                // preferred width is the minimum width
                let w = Rc::downgrade(this);
                this.layout_info.$preferred_width.set_binding(Some(move || {
                    Some(w.upgrade()?.layout_info.$minimum_width.get())
                }));

                // preferred height is the sum of preferred height
                let w = Rc::downgrade(this);
                this.layout_info.$preferred_height.set_binding(move || {
                    w.upgrade().map_or(0., |x| {
                        x.children
                            .borrow()
                            .iter()
                            .map(|i| i.layout_info().$preferred_height.get())
                            .sum()
                    })
                });

                // Set the positions
                let w = Rc::downgrade(this);
                this.positions.set_binding(move || {
                    w.upgrade().map_or(Vec::default(), |w| {
                        let v = w
                            .children
                            .borrow()
                            .iter()
                            .map(|x| {
                                layout_engine::ItemInfo {
                                    min: x.layout_info().$minimum_height.get(),
                                    max: x.layout_info().$maximum_height.get(),
                                    preferred: x.layout_info().$preferred_height.get(),
                                    expand: 1, // FIXME
                                }
                            })
                            .collect::<Vec<_>>();
                        layout_engine::do_layout(
                            &v,
                            layout_engine::compute_total_info(&v, 0.),
                            0.,
                            w.geometry.$height(),
                        )
                    })
                });

                // Set the sizes
                for (idx, x) in this.children.borrow().iter().enumerate() {
                    let w = Rc::downgrade(this);
                    x.geometry()
                        .$width
                        .set_binding(Some(move || Some(w.upgrade()?.geometry().$width())));
                    x.geometry().$x.set_binding(|| 0.);
                    let w = Rc::downgrade(this);
                    x.geometry().$height.set_binding(Some(move || {
                        Some(w.upgrade()?.positions.get().get(idx)?.size)
                    }));
                    let w = Rc::downgrade(this);
                    x.geometry().$y.set_binding(Some(move || {
                        Some(w.upgrade()?.positions.get().get(idx)?.pos)
                    }));
                }
            }
        }
    };
}

declare_box_layout!(
    ColumnLayout,
    x,
    width,
    minimum_width,
    maximum_width,
    preferred_width,
    y,
    height,
    minimum_height,
    maximum_height,
    preferred_height
);

declare_box_layout!(
    RowLayout,
    y,
    height,
    minimum_height,
    maximum_height,
    preferred_height,
    x,
    width,
    minimum_width,
    maximum_width,
    preferred_width
);

#[test]
fn test_layout() {
    #[derive(Default)]
    pub struct LItem<'a> {
        geometry: Geometry<'a>,
        layout_info: LayoutInfo<'a>,
        width: Property<'a, f64>,
        height: Property<'a, f64>,
    }
    impl<'a> Item<'a> for LItem<'a> {
        fn geometry(&self) -> &Geometry<'a> {
            &self.geometry
        }
        fn layout_info(&self) -> &LayoutInfo<'a> {
            &self.layout_info
        }
    }
    impl<'a> LItem<'a> {
        pub fn new() -> Rc<Self> {
            let r = Rc::new(LItem::default());
            let w = Rc::downgrade(&r);
            r.layout_info
                .minimum_height
                .set_binding(move || w.upgrade().map_or(0., |w| w.height.get()));
            let w = Rc::downgrade(&r);
            r.layout_info
                .preferred_height
                .set_binding(move || w.upgrade().map_or(0., |w| w.height.get()));
            let w = Rc::downgrade(&r);
            r.layout_info
                .maximum_height
                .set_binding(move || w.upgrade().map_or(0., |w| w.height.get()));
            let w = Rc::downgrade(&r);
            r.layout_info
                .minimum_width
                .set_binding(move || w.upgrade().map_or(0., |w| w.width.get()));
            let w = Rc::downgrade(&r);
            r.layout_info
                .preferred_width
                .set_binding(move || w.upgrade().map_or(0., |w| w.width.get()));
            let w = Rc::downgrade(&r);
            r.layout_info
                .maximum_width
                .set_binding(move || w.upgrade().map_or(0., |w| w.width.get()));
            r
        }
    }

    let lay = rsml! {
        ColumnLayout {
            geometry.width: ColumnLayout.layout_info.preferred_width.get(),
            geometry.height: ColumnLayout.layout_info.preferred_height.get(),
        }
    };

    lay.add_child(rsml! {
        LItem {
            width : 150.,
            height : 100.,
        }
    });
    assert_eq!(lay.geometry.width(), 150.);
    assert_eq!(lay.geometry.height(), 100.);
    let middle = rsml! {
        LItem {
            width : 110.,
            height : 90.,
        }
    };
    lay.add_child(middle.clone());
    lay.add_child(rsml! {
        LItem {
            width : 190.,
            height : 60.,
        }
    });
    assert_eq!(lay.geometry.width(), 190.);
    assert_eq!(lay.geometry.height(), 100. + 90. + 60.);

    middle.width.set(200.);
    middle.height.set(50.);

    assert_eq!(lay.geometry.width(), 200.);
    assert_eq!(lay.geometry.height(), 100. + 50. + 60.);

    assert_eq!(
        lay.geometry.height(),
        lay.children.borrow()[2].geometry().bottom()
    );
}

/// Can contains other Items, resize the items to the size of the Caintainer
#[derive(Default)]
pub struct Container<'a> {
    pub geometry: Geometry<'a>,
    pub layout_info: LayoutInfo<'a>,
    children: RefCell<Vec<Rc<dyn Item<'a> + 'a>>>,
}
impl<'a> Item<'a> for Container<'a> {
    fn geometry(&self) -> &Geometry<'a> {
        &self.geometry
    }
    fn layout_info(&self) -> &LayoutInfo<'a> {
        &self.layout_info
    }

    fn draw(&self, rc: &mut Piet) -> DrawResult {
        let g = self.geometry().to_rect();
        rc.with_save(|rc| {
            rc.transform(piet_common::kurbo::Affine::translate(g.origin().to_vec2()));
            for child in self.children.borrow().iter() {
                child.draw(rc)?
            }
            Ok(())
        })
    }

    fn mouse_event(&self, event: MouseEvent) -> bool {
        let mut ret = false;
        for i in self.children.borrow().iter() {
            ret = ret || i.mouse_event(event);
        }
        ret
    }
}

impl<'a> ItemContainer<'a> for Rc<Container<'a>> {
    fn add_child(&self, child: Rc<dyn Item<'a> + 'a>) {
        self.children.borrow_mut().push(child);
        Container::build_layout(self);
    }
}

impl<'a> Container<'a> {
    pub fn new() -> Rc<Self> {
        Default::default()
    }

    fn build_layout(this: &Rc<Self>) {
        for x in this.children.borrow().iter() {
            let w = Rc::downgrade(this);
            x.geometry()
                .width
                .set_binding(Some(move || Some(w.upgrade()?.geometry().width())));
            let w = Rc::downgrade(this);
            x.geometry()
                .height
                .set_binding(Some(move || Some(w.upgrade()?.geometry().height())));
            x.geometry().x.set(0.);
            x.geometry().y.set(0.);
        }
    }
}



/// Can contains other Items, resize the items to the size of the Caintainer
#[derive(Default)]
pub struct FreeLayout<'a> {
    pub geometry: Geometry<'a>,
    pub layout_info: LayoutInfo<'a>,
    children: RefCell<Vec<Rc<dyn Item<'a> + 'a>>>,
}
impl<'a> Item<'a> for FreeLayout<'a> {
    fn geometry(&self) -> &Geometry<'a> {
        &self.geometry
    }
    fn layout_info(&self) -> &LayoutInfo<'a> {
        &self.layout_info
    }

    fn draw(&self, rc: &mut Piet) -> DrawResult {
        let g = self.geometry().to_rect();
        rc.with_save(|rc| {
            rc.transform(piet_common::kurbo::Affine::translate(g.origin().to_vec2()));
            for child in self.children.borrow().iter() {
                child.draw(rc)?
            }
            Ok(())
        })
    }


    fn mouse_event(&self, event: MouseEvent) -> bool {
        for i in self.children.borrow().iter() {
            let g = i.geometry().to_rect();
            if g.contains(event.position()) {
                return i.mouse_event(event.translated(g.origin()));
            }
        }
        return false;
    }

}

impl<'a> ItemContainer<'a> for Rc<FreeLayout<'a>> {
    fn add_child(&self, child: Rc<dyn Item<'a> + 'a>) {
        self.children.borrow_mut().push(child);
    }
}

impl<'a> FreeLayout<'a> {
    pub fn new() -> Rc<Self> {
        Default::default()
    }
}

#[derive(Clone)]
pub struct QColor(piet_common::Color);
impl Default for QColor {
    fn default() -> Self { Self(piet_common::Color::WHITE) }
}
impl From<u32> for QColor {
    fn from(val : u32) -> Self { Self(piet_common::Color::from_rgba32_u32(val.swap_bytes())) }
}

#[derive(Default)]
pub struct Rectangle<'a> {
    pub geometry: Geometry<'a>,
    pub layout_info: LayoutInfo<'a>,
    pub color: Property<'a, QColor>,
}

impl<'a> Item<'a> for Rectangle<'a> {
    fn geometry(&self) -> &Geometry<'a> {
        &self.geometry
    }
    fn layout_info(&self) -> &LayoutInfo<'a> {
        &self.layout_info
    }

    fn draw(&self, rc: &mut Piet) -> DrawResult {
        let g = self.geometry().to_rect();
        let b = rc.solid_brush(self.color.get().0);
        rc.fill(g, &b);
        Ok(())
    }
}
impl<'a> Rectangle<'a> {
    pub fn new() -> Rc<Self> {
        Default::default()
    }
}


/// constants that follow Qt::Alignment
pub mod alignment {
    pub const LEFT: i32 = 1;
    pub const RIGHT: i32 = 2;
    pub const HCENTER: i32 = 4;
    pub const JUSTIFY: i32 = 8;
    pub const TOP: i32 = 32;
    pub const BOTTOM: i32 = 64;
    pub const VCENTER: i32 = 128;

}

#[derive(Default)]
pub struct Text<'a> {
    pub geometry: Geometry<'a>,
    pub layout_info: LayoutInfo<'a>,
    pub text: Property<'a, String>,
    pub vertical_alignment: Property<'a, i32>,
    pub horizontal_alignment: Property<'a, i32>,
    pub color: Property<'a, QColor>,
}

impl<'a> Item<'a> for Text<'a> {
    fn geometry(&self) -> &Geometry<'a> {
        &self.geometry
    }
    fn layout_info(&self) -> &LayoutInfo<'a> {
        &self.layout_info
    }

    fn draw(&self, rc: &mut Piet) -> DrawResult {
        use piet_common::{Text, TextLayoutBuilder, FontBuilder};
        let t = rc.text();
        let f = t.new_font_by_name("", 30.)?.build()?;
        let lay = t.new_text_layout(&f, &self.text.get())?.build()?;
        let b = rc.solid_brush(self.color.get().0);
        let pos = self.geometry().to_rect().center(); // FIXME
        rc.draw_text(&lay , pos , &b);
        Ok(())
    }

}
impl<'a> Text<'a> {
    pub fn new() -> Rc<Self> {
        let t = Rc::<Self>::default();
        t.color.set(QColor::from(0xff000000));
        t
    }
}

/// Similar to a QtQuick MouseArea
#[derive(Default)]
pub struct MouseArea<'a> {
    pub geometry: Geometry<'a>,
    pub layout_info: LayoutInfo<'a>,
    pub pressed: Property<'a, bool>,
    pub on_clicked: Signal<'a>,
}

impl<'a> Item<'a> for MouseArea<'a> {
    fn geometry(&self) -> &Geometry<'a> {
        &self.geometry
    }
    fn layout_info(&self) -> &LayoutInfo<'a> {
        &self.layout_info
    }
    fn mouse_event(&self, event: MouseEvent) -> bool {
        match event {
            MouseEvent::Press(_) => self.pressed.set(true),
            MouseEvent::Release(_) => {
                self.pressed.set(false);
                self.on_clicked.emit();
            }
            _ => {}
        }
        true
    }
}
impl<'a> MouseArea<'a> {
    pub fn new() -> Rc<Self> {
        Default::default()
    }
}


#[derive(Default)]
pub struct Image<'a> {
    pub geometry: Geometry<'a>,
    pub layout_info: LayoutInfo<'a>,
    pub image: Property<'a, Option<image::DynamicImage>>,
}

impl<'a> Item<'a> for Image<'a> {
    fn geometry(&self) -> &Geometry<'a> {
        &self.geometry
    }
    fn layout_info(&self) -> &LayoutInfo<'a> {
        &self.layout_info
    }

    fn draw(&self, rc: &mut Piet) -> DrawResult {
        let g = self.geometry().to_rect();
        if g.area() <= 0. {
            return Ok(());
        }
        if let Some(im) = self.image.get() {
            let im = im.to_rgba();
            let im = rc.make_image(
                im.width() as _,
                im.height() as _,
                &im.into_raw(),
                piet_common::ImageFormat::RgbaSeparate,
            )?;
            rc.draw_image(&im, g, piet_common::InterpolationMode::NearestNeighbor);
        }
        Ok(())
    }

}
impl<'a> Image<'a> {
    pub fn new() -> Rc<Self> {
        let t = Rc::<Self>::default();
        t
    }
}
