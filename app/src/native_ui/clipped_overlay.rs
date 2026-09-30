use std::{cell::Cell, rc::Rc};

use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::{
    AnyElement, App, Bounds, ContentMask, Element, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, ScrollHandle, Window, div, prelude::*, px,
};

use super::theme;

// GPUI's deferred draw restores clipping for paint but not prepaint. Restore it
// around the child too, so off-viewport controls cannot register clickable hitboxes.
pub(super) fn clipped_overlay(child: impl IntoElement) -> impl IntoElement {
    let mask = Rc::new(Cell::new(None));
    ClippedOverlay {
        child: Some(
            MaskedOverlay {
                child: child.into_any_element(),
                mask: mask.clone(),
            }
            .into_any_element(),
        ),
        mask,
    }
}

// Pin the overlay to the viewport rather than the end of its scrolling content.
// Deferring keeps the thumb above insertion targets without enlarging the scroll range.
pub(super) fn vertical_scrollbar_overlay(handle: &ScrollHandle) -> impl IntoElement {
    clipped_overlay(
        div().absolute().inset_0().child(
            Scrollbar::vertical(handle)
                .styles(|styles| styles.track(|style| style.width(px(theme::SCROLLBAR_WIDTH)))),
        ),
    )
}

struct ClippedOverlay {
    child: Option<AnyElement>,
    mask: Rc<Cell<Option<ContentMask<Pixels>>>>,
}

impl Element for ClippedOverlay {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui_kit::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.as_mut().unwrap().request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        let mask = window.content_mask();
        self.mask.set(Some(mask));
        window.defer_draw(
            self.child.take().unwrap(),
            window.element_offset(),
            0,
            Some(mask),
        );
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }
}

impl IntoElement for ClippedOverlay {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

struct MaskedOverlay {
    child: AnyElement,
    mask: Rc<Cell<Option<ContentMask<Pixels>>>>,
}

impl Element for MaskedOverlay {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui_kit::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(self.mask.get(), |window| {
            self.child.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(self.mask.get(), |window| self.child.paint(window, cx));
    }
}

impl IntoElement for MaskedOverlay {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
