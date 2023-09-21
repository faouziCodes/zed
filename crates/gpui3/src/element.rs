use super::{Handle, Layout, LayoutId, Pixels, Point, Result, ViewContext};
pub(crate) use smallvec::SmallVec;
use std::{any::Any, cell::RefCell, marker::PhantomData, rc::Rc};

pub trait Element: 'static {
    type State;
    type FrameState;

    fn layout(
        &mut self,
        state: &mut Self::State,
        cx: &mut ViewContext<Self::State>,
    ) -> Result<(LayoutId, Self::FrameState)>;

    fn paint(
        &mut self,
        layout: Layout,
        state: &mut Self::State,
        frame_state: &mut Self::FrameState,
        cx: &mut ViewContext<Self::State>,
    ) -> Result<()>;
}

pub trait ParentElement<S> {
    fn children_mut(&mut self) -> &mut SmallVec<[AnyElement<S>; 2]>;

    fn child(mut self, child: impl IntoAnyElement<S>) -> Self
    where
        Self: Sized,
    {
        self.children_mut().push(child.into_any());
        self
    }

    fn children(mut self, iter: impl IntoIterator<Item = impl IntoAnyElement<S>>) -> Self
    where
        Self: Sized,
    {
        self.children_mut()
            .extend(iter.into_iter().map(|item| item.into_any()));
        self
    }
}

trait ElementObject<S> {
    fn layout(&mut self, state: &mut S, cx: &mut ViewContext<S>) -> Result<LayoutId>;
    fn paint(
        &mut self,
        state: &mut S,
        offset: Option<Point<Pixels>>,
        cx: &mut ViewContext<S>,
    ) -> Result<()>;
}

struct RenderedElement<E: Element> {
    element: E,
    phase: ElementRenderPhase<E::FrameState>,
}

#[derive(Default)]
enum ElementRenderPhase<S> {
    #[default]
    Rendered,
    LayoutRequested {
        layout_id: LayoutId,
        frame_state: S,
    },
    Painted {
        layout: Layout,
        frame_state: S,
    },
}

/// Internal struct that wraps an element to store Layout and FrameState after the element is rendered.
/// It's allocated as a trait object to erase the element type and wrapped in AnyElement<E::State> for
/// improved usability.
impl<E: Element> RenderedElement<E> {
    fn new(element: E) -> Self {
        RenderedElement {
            element,
            phase: ElementRenderPhase::Rendered,
        }
    }
}

impl<E: Element> ElementObject<E::State> for RenderedElement<E> {
    fn layout(&mut self, state: &mut E::State, cx: &mut ViewContext<E::State>) -> Result<LayoutId> {
        let (layout_id, frame_state) = self.element.layout(state, cx)?;
        self.phase = ElementRenderPhase::LayoutRequested {
            layout_id,
            frame_state,
        };
        Ok(layout_id)
    }

    fn paint(
        &mut self,
        state: &mut E::State,
        offset: Option<Point<Pixels>>,
        cx: &mut ViewContext<E::State>,
    ) -> Result<()> {
        self.phase = match std::mem::take(&mut self.phase) {
            ElementRenderPhase::Rendered => panic!("must call layout before paint"),

            ElementRenderPhase::LayoutRequested {
                layout_id,
                mut frame_state,
            } => {
                let mut layout = cx.layout(layout_id)?.clone();
                offset.map(|offset| layout.bounds.origin += offset);
                self.element
                    .paint(layout.clone(), state, &mut frame_state, cx)?;
                ElementRenderPhase::Painted {
                    layout,
                    frame_state,
                }
            }

            ElementRenderPhase::Painted {
                layout,
                mut frame_state,
            } => {
                self.element
                    .paint(layout.clone(), state, &mut frame_state, cx)?;
                ElementRenderPhase::Painted {
                    layout,
                    frame_state,
                }
            }
        };

        Ok(())
    }
}

pub struct AnyElement<S>(Box<dyn ElementObject<S>>);

impl<S> AnyElement<S> {
    pub fn layout(&mut self, state: &mut S, cx: &mut ViewContext<S>) -> Result<LayoutId> {
        self.0.layout(state, cx)
    }

    pub fn paint(
        &mut self,
        state: &mut S,
        offset: Option<Point<Pixels>>,
        cx: &mut ViewContext<S>,
    ) -> Result<()> {
        self.0.paint(state, offset, cx)
    }
}

pub trait IntoAnyElement<S> {
    fn into_any(self) -> AnyElement<S>;
}

impl<E: Element> IntoAnyElement<E::State> for E {
    fn into_any(self) -> AnyElement<E::State> {
        AnyElement(Box::new(RenderedElement::new(self)))
    }
}

impl<S> IntoAnyElement<S> for AnyElement<S> {
    fn into_any(self) -> AnyElement<S> {
        self
    }
}
