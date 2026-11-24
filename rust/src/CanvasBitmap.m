#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#import <objc/runtime.h>

#include <math.h>
#include <stdbool.h>

static char tableLayersKey;

typedef void (*TMViewPaint)(NSView *, NSRect);

static CGImageRef create_view_image(NSView *view, NSRect area, CGFloat scale, TMViewPaint paint) {
    size_t width = (size_t)ceil(NSWidth(area) * scale);
    size_t height = (size_t)ceil(NSHeight(area) * scale);
    static CGColorSpaceRef color;
    static dispatch_once_t once;
    dispatch_once(&once, ^{ color = CGColorSpaceCreateWithName(kCGColorSpaceSRGB); });
    if (!color) return NULL;
    CGContextRef cg = CGBitmapContextCreate(NULL, width, height, 8, 0, color,
        kCGBitmapByteOrder32Little | kCGImageAlphaNoneSkipFirst);
    if (!cg) return NULL;

    CGContextTranslateCTM(cg, 0, (CGFloat)height);
    CGContextScaleCTM(cg, scale, -scale);
    CGContextTranslateCTM(cg, -NSMinX(area), -NSMinY(area));
    CGContextClipToRect(cg, area);
    [NSGraphicsContext saveGraphicsState];
    NSGraphicsContext.currentContext = [NSGraphicsContext graphicsContextWithCGContext:cg flipped:YES];
    paint(view, area);
    [NSGraphicsContext restoreGraphicsState];

    CGImageRef image = CGBitmapContextCreateImage(cg);
    CGContextRelease(cg);
    return image;
}

bool tm_paint_view_bitmap(NSView *view, NSRect dirty, TMViewPaint paint) {
    NSRect area = NSIntersectionRect(dirty, view.bounds);
    if (NSIsEmptyRect(area)) return true;
    CALayer *base = view.layer;
    if (!base) return false;
    CGFloat scale = view.window ? view.window.backingScaleFactor : 1.0;
    bool full = NSEqualRects(area, view.bounds);
    // Preserve edge pixels when a column has been scrolled or resized fractionally.
    NSRect pixels = full ? area : [view backingAlignedRect:area options:NSAlignAllEdgesOutward];
    CGImageRef image = create_view_image(view, pixels, scale, paint);
    if (!image) return false;

    NSMutableDictionary<NSValue *, CALayer *> *layers = objc_getAssociatedObject(view, &tableLayersKey);
    CALayer *destination = base;
    [CATransaction begin];
    [CATransaction setDisableActions:YES];
    if (full) {
        for (CALayer *tile in layers.allValues) [tile removeFromSuperlayer];
        [layers removeAllObjects];
    } else {
        if (!layers) {
            layers = [NSMutableDictionary dictionary];
            objc_setAssociatedObject(view, &tableLayersKey, layers, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
        }
        NSValue *key = [NSValue valueWithRect:area];
        destination = layers[key];
        if (!destination) {
            destination = [CALayer layer];
            destination.opaque = YES;
            destination.frame = [view convertRectToLayer:pixels];
            [base addSublayer:destination];
            layers[key] = destination;
        }
    }
    destination.contentsScale = scale;
    destination.contents = (__bridge id)image;
    [CATransaction commit];
    CGImageRelease(image);
    return true;
}
