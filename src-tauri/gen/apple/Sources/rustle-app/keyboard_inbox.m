#import <AVFoundation/AVFoundation.h>
#import <Foundation/Foundation.h>
#import <UIKit/UIKit.h>

static NSString *const kRustleAppGroup = @"group.com.annix.rustle";
static NSString *const kRustleStopName = @"com.annix.rustle.keyboard-stop";
static void (*gKeyboardStopHandler)(void) = NULL;
static BOOL gStopObserverAdded = NO;
static UIBackgroundTaskIdentifier gTranscribeTask = UIBackgroundTaskInvalid;

static NSUserDefaults *rustle_group_defaults(void) {
    return [[NSUserDefaults alloc] initWithSuiteName:kRustleAppGroup];
}

void rustle_publish_keyboard_transcript(const char *text) {
    if (text == NULL) {
        return;
    }
    NSUserDefaults *defaults = rustle_group_defaults();
    if (defaults == nil) {
        return;
    }
    NSString *value = [NSString stringWithUTF8String:text];
    if (value == nil) {
        return;
    }
    [defaults setObject:value forKey:@"pendingText"];
    [defaults setObject:[[NSUUID UUID] UUIDString] forKey:@"pendingToken"];
    [defaults setObject:@"idle" forKey:@"phase"];
    [defaults synchronize];
}

void rustle_set_keyboard_phase(const char *phase) {
    NSUserDefaults *defaults = rustle_group_defaults();
    if (defaults == nil || phase == NULL) {
        return;
    }
    NSString *value = [NSString stringWithUTF8String:phase];
    if (value == nil) {
        return;
    }
    [defaults setObject:value forKey:@"phase"];
    [defaults synchronize];
}

void rustle_return_to_host_app(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        UIApplication *app = [UIApplication sharedApplication];
        SEL suspend = NSSelectorFromString(@"suspend");
        if ([app respondsToSelector:suspend]) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
            [app performSelector:suspend];
#pragma clang diagnostic pop
        }
    });
}

void rustle_prepare_phone_audio_session(void) {
    AVAudioSession *session = [AVAudioSession sharedInstance];
    NSError *error = nil;
    [session setCategory:AVAudioSessionCategoryPlayAndRecord
                    mode:AVAudioSessionModeSpokenAudio
                 options:AVAudioSessionCategoryOptionMixWithOthers |
                         AVAudioSessionCategoryOptionAllowBluetooth |
                         AVAudioSessionCategoryOptionDefaultToSpeaker
                   error:&error];
    [session setActive:YES error:&error];
}

static void rustle_keyboard_stop_callback(
    CFNotificationCenterRef center,
    void *observer,
    CFNotificationName name,
    const void *object,
    CFDictionaryRef userInfo
) {
    (void)center;
    (void)observer;
    (void)name;
    (void)object;
    (void)userInfo;
    if (gKeyboardStopHandler != NULL) {
        gKeyboardStopHandler();
    }
}

void rustle_listen_for_keyboard_stop(void (*handler)(void)) {
    gKeyboardStopHandler = handler;
    if (gStopObserverAdded) {
        return;
    }
    gStopObserverAdded = YES;
    CFNotificationCenterAddObserver(
        CFNotificationCenterGetDarwinNotifyCenter(),
        NULL,
        rustle_keyboard_stop_callback,
        (__bridge CFStringRef)kRustleStopName,
        NULL,
        CFNotificationSuspensionBehaviorDeliverImmediately
    );
}

void rustle_begin_transcribe_background_task(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        if (gTranscribeTask != UIBackgroundTaskInvalid) {
            return;
        }
        gTranscribeTask = [[UIApplication sharedApplication]
            beginBackgroundTaskWithName:@"rustle-transcribe"
                      expirationHandler:^{
                          UIBackgroundTaskIdentifier task = gTranscribeTask;
                          gTranscribeTask = UIBackgroundTaskInvalid;
                          if (task != UIBackgroundTaskInvalid) {
                              [[UIApplication sharedApplication] endBackgroundTask:task];
                          }
                      }];
    });
}

void rustle_end_transcribe_background_task(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        UIBackgroundTaskIdentifier task = gTranscribeTask;
        gTranscribeTask = UIBackgroundTaskInvalid;
        if (task != UIBackgroundTaskInvalid) {
            [[UIApplication sharedApplication] endBackgroundTask:task];
        }
    });
}
