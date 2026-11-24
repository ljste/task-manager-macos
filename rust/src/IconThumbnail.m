#import <AppKit/AppKit.h>
#import <ImageIO/ImageIO.h>

NS_RETURNS_RETAINED NSImage *tm_icon_thumbnail(NSString *path, NSUInteger pixels) {
    CGImageSourceRef source = CGImageSourceCreateWithURL((__bridge CFURLRef)[NSURL fileURLWithPath:path], NULL);
    if (!source) return nil;

    size_t best = 0;
    NSUInteger bestWidth = 0;
    for (size_t i = 0; i < CGImageSourceGetCount(source); i++) {
        NSDictionary *properties = CFBridgingRelease(CGImageSourceCopyPropertiesAtIndex(source, i, NULL));
        NSUInteger width = [properties[(__bridge NSString *)kCGImagePropertyPixelWidth] unsignedIntegerValue];
        if (width == 0) continue;
        if (width == pixels) {
            best = i;
            break;
        }
        if ((bestWidth < pixels && width > bestWidth) || (width >= pixels && width < bestWidth)) {
            best = i;
            bestWidth = width;
        }
    }

    NSDictionary *options = @{
        (__bridge NSString *)kCGImageSourceThumbnailMaxPixelSize: @(pixels),
        (__bridge NSString *)kCGImageSourceCreateThumbnailFromImageAlways: @YES,
        (__bridge NSString *)kCGImageSourceShouldCacheImmediately: @YES,
    };
    CGImageRef thumbnail = CGImageSourceCreateThumbnailAtIndex(source, best, (__bridge CFDictionaryRef)options);
    CFRelease(source);
    if (!thumbnail) return nil;
    NSImage *icon = [[NSImage alloc] initWithCGImage:thumbnail size:NSMakeSize(pixels / 2.0, pixels / 2.0)];
    CGImageRelease(thumbnail);
    return icon;
}
