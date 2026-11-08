# \PlayerApi

All URIs are relative to *https://api.spotify.com/v1*

Method | HTTP request | Description
------------- | ------------- | -------------
[**add_to_queue**](PlayerApi.md#add_to_queue) | **POST** /me/player/queue | Add Item to Playback Queue 
[**get_a_users_available_devices**](PlayerApi.md#get_a_users_available_devices) | **GET** /me/player/devices | Get Available Devices 
[**get_information_about_the_users_current_playback**](PlayerApi.md#get_information_about_the_users_current_playback) | **GET** /me/player | Get Playback State 
[**get_queue**](PlayerApi.md#get_queue) | **GET** /me/player/queue | Get the User's Queue 
[**get_recently_played**](PlayerApi.md#get_recently_played) | **GET** /me/player/recently-played | Get Recently Played Tracks 
[**get_the_users_currently_playing_track**](PlayerApi.md#get_the_users_currently_playing_track) | **GET** /me/player/currently-playing | Get Currently Playing Track 
[**pause_a_users_playback**](PlayerApi.md#pause_a_users_playback) | **PUT** /me/player/pause | Pause Playback 
[**seek_to_position_in_currently_playing_track**](PlayerApi.md#seek_to_position_in_currently_playing_track) | **PUT** /me/player/seek | Seek To Position 
[**set_repeat_mode_on_users_playback**](PlayerApi.md#set_repeat_mode_on_users_playback) | **PUT** /me/player/repeat | Set Repeat Mode 
[**set_volume_for_users_playback**](PlayerApi.md#set_volume_for_users_playback) | **PUT** /me/player/volume | Set Playback Volume 
[**skip_users_playback_to_next_track**](PlayerApi.md#skip_users_playback_to_next_track) | **POST** /me/player/next | Skip To Next 
[**skip_users_playback_to_previous_track**](PlayerApi.md#skip_users_playback_to_previous_track) | **POST** /me/player/previous | Skip To Previous 
[**start_a_users_playback**](PlayerApi.md#start_a_users_playback) | **PUT** /me/player/play | Start/Resume Playback 
[**toggle_shuffle_for_users_playback**](PlayerApi.md#toggle_shuffle_for_users_playback) | **PUT** /me/player/shuffle | Toggle Playback Shuffle 
[**transfer_a_users_playback**](PlayerApi.md#transfer_a_users_playback) | **PUT** /me/player | Transfer Playback 



## add_to_queue

> add_to_queue(uri, device_id)
Add Item to Playback Queue 

Add an item to be played next in the user's current playback queue. This API only works for users who have Spotify Premium. The order of execution is not guaranteed when you use this API with other Player API endpoints. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**uri** | **String** |  | [required] |
**device_id** | Option<**String**> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_a_users_available_devices

> models::GetAUsersAvailableDevices200Response get_a_users_available_devices()
Get Available Devices 

Get information about a user’s available Spotify Connect devices. Some device models are not supported and will not be listed in the API response. 

### Parameters

This endpoint does not need any parameter.

### Return type

[**models::GetAUsersAvailableDevices200Response**](get_a_users_available_devices_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_information_about_the_users_current_playback

> models::CurrentlyPlayingContextObject get_information_about_the_users_current_playback(market, additional_types)
Get Playback State 

Get information about the user’s current playback state, including track or episode, progress, and active device. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**market** | Option<**String**> |  |  |
**additional_types** | Option<**String**> |  |  |

### Return type

[**models::CurrentlyPlayingContextObject**](CurrentlyPlayingContextObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_queue

> models::QueueObject get_queue()
Get the User's Queue 

Get the list of objects that make up the user's queue. 

### Parameters

This endpoint does not need any parameter.

### Return type

[**models::QueueObject**](QueueObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_recently_played

> models::CursorPagingPlayHistoryObject get_recently_played(limit, after, before)
Get Recently Played Tracks 

Get tracks from the current user's recently played tracks. _**Note**: Currently doesn't support podcast episodes._ 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**limit** | Option<**i32**> |  |  |[default to 20]
**after** | Option<**i64**> |  |  |
**before** | Option<**i64**> |  |  |

### Return type

[**models::CursorPagingPlayHistoryObject**](CursorPagingPlayHistoryObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_the_users_currently_playing_track

> models::CurrentlyPlayingContextObject get_the_users_currently_playing_track(market, additional_types)
Get Currently Playing Track 

Get the object currently being played on the user's Spotify account. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**market** | Option<**String**> |  |  |
**additional_types** | Option<**String**> |  |  |

### Return type

[**models::CurrentlyPlayingContextObject**](CurrentlyPlayingContextObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## pause_a_users_playback

> pause_a_users_playback(device_id)
Pause Playback 

Pause playback on the user's account. This API only works for users who have Spotify Premium. The order of execution is not guaranteed when you use this API with other Player API endpoints. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**device_id** | Option<**String**> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## seek_to_position_in_currently_playing_track

> seek_to_position_in_currently_playing_track(position_ms, device_id)
Seek To Position 

Seeks to the given position in the user’s currently playing track. This API only works for users who have Spotify Premium. The order of execution is not guaranteed when you use this API with other Player API endpoints. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**position_ms** | **i32** |  | [required] |
**device_id** | Option<**String**> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## set_repeat_mode_on_users_playback

> set_repeat_mode_on_users_playback(state, device_id)
Set Repeat Mode 

Set the repeat mode for the user's playback. This API only works for users who have Spotify Premium. The order of execution is not guaranteed when you use this API with other Player API endpoints. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**state** | **String** |  | [required] |
**device_id** | Option<**String**> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## set_volume_for_users_playback

> set_volume_for_users_playback(volume_percent, device_id)
Set Playback Volume 

Set the volume for the user’s current playback device. This API only works for users who have Spotify Premium. The order of execution is not guaranteed when you use this API with other Player API endpoints. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**volume_percent** | **i32** |  | [required] |
**device_id** | Option<**String**> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## skip_users_playback_to_next_track

> skip_users_playback_to_next_track(device_id)
Skip To Next 

Skips to next track in the user’s queue. This API only works for users who have Spotify Premium. The order of execution is not guaranteed when you use this API with other Player API endpoints. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**device_id** | Option<**String**> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## skip_users_playback_to_previous_track

> skip_users_playback_to_previous_track(device_id)
Skip To Previous 

Skips to previous track in the user’s queue. This API only works for users who have Spotify Premium. The order of execution is not guaranteed when you use this API with other Player API endpoints. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**device_id** | Option<**String**> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## start_a_users_playback

> start_a_users_playback(device_id, start_a_users_playback_request)
Start/Resume Playback 

Start a new context or resume current playback on the user's active device. This API only works for users who have Spotify Premium. The order of execution is not guaranteed when you use this API with other Player API endpoints. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**device_id** | Option<**String**> |  |  |
**start_a_users_playback_request** | Option<[**StartAUsersPlaybackRequest**](StartAUsersPlaybackRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## toggle_shuffle_for_users_playback

> toggle_shuffle_for_users_playback(state, device_id)
Toggle Playback Shuffle 

Toggle shuffle on or off for user’s playback. This API only works for users who have Spotify Premium. The order of execution is not guaranteed when you use this API with other Player API endpoints. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**state** | **bool** |  | [required] |
**device_id** | Option<**String**> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## transfer_a_users_playback

> transfer_a_users_playback(transfer_a_users_playback_request)
Transfer Playback 

Transfer playback to a new device and optionally begin playback. This API only works for users who have Spotify Premium. The order of execution is not guaranteed when you use this API with other Player API endpoints. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**transfer_a_users_playback_request** | Option<[**TransferAUsersPlaybackRequest**](TransferAUsersPlaybackRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

