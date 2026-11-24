#import <CoreServices/CoreServices.h>
#import <Foundation/Foundation.h>
#import <ServiceManagement/ServiceManagement.h>

static NSString *const disabledPrefix = @"TaskManagerDisabledLoginItem:";

static NSArray *copy_items(LSSharedFileListRef list) {
    return CFBridgingRelease(LSSharedFileListCopySnapshot(list, NULL));
}

static NSString *item_path(LSSharedFileListItemRef item) {
    NSURL *url = CFBridgingRelease(LSSharedFileListItemCopyResolvedURL(item,
        kLSSharedFileListNoUserInteraction | kLSSharedFileListDoNotMountVolumes, NULL));
    return url.isFileURL ? url.URLByStandardizingPath.path : nil;
}

static LSSharedFileListItemRef find_item(NSArray *snapshot, NSString *path) {
    for (id entry in snapshot) {
        LSSharedFileListItemRef item = (__bridge LSSharedFileListItemRef)entry;
        if ([item_path(item) isEqualToString:path]) return item;
    }
    return NULL;
}

NS_RETURNS_RETAINED NSDictionary<NSString *, NSNumber *> *tm_copy_login_items(void) {
    @autoreleasepool {
        LSSharedFileListRef list = LSSharedFileListCreate(NULL, kLSSharedFileListSessionLoginItems, NULL);
        if (!list) return nil;
        NSArray *snapshot = copy_items(list);
        CFRelease(list);
        if (!snapshot) return nil;

        NSMutableDictionary *items = [NSMutableDictionary dictionary];
        for (NSString *key in NSUserDefaults.standardUserDefaults.dictionaryRepresentation) {
            if ([key hasPrefix:disabledPrefix]) {
                NSString *path = [key substringFromIndex:disabledPrefix.length];
                if ([NSFileManager.defaultManager fileExistsAtPath:path]) items[path] = @NO;
            }
        }
        for (id entry in snapshot) {
            NSString *path = item_path((__bridge LSSharedFileListItemRef)entry);
            if (path) items[path] = @YES;
        }
        return items;
    }
}

NS_RETURNS_RETAINED NSString *tm_set_login_item(NSString *path, bool enable) {
    @autoreleasepool {
        LSSharedFileListRef list = LSSharedFileListCreate(NULL, kLSSharedFileListSessionLoginItems, NULL);
        if (!list) return @"Open at Login is unavailable. Open Login Items in System Settings.";
        __attribute__((objc_precise_lifetime)) NSArray *snapshot = copy_items(list);
        LSSharedFileListItemRef item = find_item(snapshot, path);
        NSUserDefaults *defaults = NSUserDefaults.standardUserDefaults;
        NSString *key = [disabledPrefix stringByAppendingString:path];
        NSNumber *savedHidden = [defaults objectForKey:key];
        NSString *error = nil;

        if (!snapshot) {
            error = @"Could not read Open at Login.";
        } else if (enable && !item) {
            if (!savedHidden || ![NSFileManager.defaultManager fileExistsAtPath:path]) {
                error = @"This login item has moved or is no longer available.";
            } else {
                NSDictionary *properties = @{(__bridge NSString *)kLSSharedFileListLoginItemHidden: savedHidden};
                LSSharedFileListItemRef added = LSSharedFileListInsertItemURL(list, kLSSharedFileListItemLast,
                    NULL, NULL, (__bridge CFURLRef)[NSURL fileURLWithPath:path], (__bridge CFDictionaryRef)properties, NULL);
                if (added) CFRelease(added);
                else error = @"macOS could not add this item to Open at Login.";
            }
        } else if (!enable && item) {
            NSNumber *hidden = CFBridgingRelease(LSSharedFileListItemCopyProperty(item, kLSSharedFileListLoginItemHidden));
            [defaults setBool:hidden.boolValue forKey:key];
            OSStatus status = LSSharedFileListItemRemove(list, item);
            if (status != noErr) error = [NSString stringWithFormat:@"macOS could not remove this login item (error %d).", (int)status];
        } else if (!enable && !savedHidden) {
            error = @"The login item was already removed outside Task Manager.";
        }

        if (!error) {
            NSArray *updated = copy_items(list);
            if (!updated || (find_item(updated, path) != NULL) != enable)
                error = @"macOS did not confirm the change. Open Login Items in System Settings.";
        }
        if (!error && enable) [defaults removeObjectForKey:key];
        if (error && !enable && !savedHidden && find_item(copy_items(list), path))
            [defaults removeObjectForKey:key];
        CFRelease(list);
        return error;
    }
}

bool tm_legacy_requires_approval(NSString *path) {
    return [SMAppService statusForLegacyURL:[NSURL fileURLWithPath:path]] == SMAppServiceStatusRequiresApproval;
}
