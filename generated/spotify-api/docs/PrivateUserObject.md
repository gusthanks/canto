# PrivateUserObject

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**account_id** | Option<**String**> | A public, immutable, pseudoanonymous identifier for the user's account. Use this field for account linking rather than the `id` field, as it is stable and will not change over the lifetime of the account.  | [optional]
**country** | Option<**String**> | The country of the user, as set in the user's account profile. An [ISO 3166-1 alpha-2 country code](http://en.wikipedia.org/wiki/ISO_3166-1_alpha-2). _This field is only available when the current user has granted access to the [user-read-private](/documentation/web-api/concepts/scopes/#list-of-scopes) scope._  | [optional]
**display_name** | Option<**String**> | The name displayed on the user's profile. `null` if not available.  | [optional]
**email** | Option<**String**> | The user's email address, as entered by the user when creating their account. _**Important!** This email address is unverified; there is no proof that it actually belongs to the user._ _This field is only available when the current user has granted access to the [user-read-email](/documentation/web-api/concepts/scopes/#list-of-scopes) scope._  | [optional]
**explicit_content** | Option<[**models::ExplicitContentSettingsObject**](ExplicitContentSettingsObject.md)> | The user's explicit content settings. _This field is only available when the current user has granted access to the [user-read-private](/documentation/web-api/concepts/scopes/#list-of-scopes) scope._  | [optional]
**external_urls** | Option<[**models::ExternalUrlObject**](ExternalUrlObject.md)> | Known external URLs for this user. | [optional]
**followers** | Option<[**models::FollowersObject**](FollowersObject.md)> | Information about the followers of the user. | [optional]
**href** | Option<**String**> | A link to the Web API endpoint for this user.  | [optional]
**id** | Option<**String**> | The [Spotify user ID](/documentation/web-api/concepts/spotify-uris-ids) for the user. Do not use this field for account linking — use `account_id` instead, which is immutable.  | [optional]
**images** | Option<[**Vec<models::ImageObject>**](ImageObject.md)> | The user's profile image. | [optional]
**product** | Option<**String**> | The user's Spotify subscription level: \"premium\", \"free\", etc. (The subscription level \"open\" can be considered the same as \"free\".) _This field is only available when the current user has granted access to the [user-read-private](/documentation/web-api/concepts/scopes/#list-of-scopes) scope._  | [optional]
**r#type** | Option<**String**> | The object type: \"user\"  | [optional]
**uri** | Option<**String**> | The [Spotify URI](/documentation/web-api/concepts/spotify-uris-ids) for the user.  | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


